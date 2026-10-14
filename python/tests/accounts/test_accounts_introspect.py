import base64
import inspect
import json

import httpx
import pytest

from vigrise.accounts import (
    SSO_HEADER,
    AccountsMisconfiguredError,
    AccountsUnavailableError,
    AsyncSsoIntrospector,
    InactiveSessionError,
    SsoIntrospector,
    sso_token_hash,
)

ACTIVE = {
    "active": True,
    "identity": {"id": "11111111-1111-1111-1111-111111111111", "email": "a@example.com", "name": "Ada", "picture": None, "email_verified": True},
    "session": {"id": "sid-1", "authenticated_at": "2026-10-11T10:00:00Z", "aal": "aal1", "expires_at": "2026-10-12T10:00:00Z"},
    "accounts": [{"session_id": "sid-1", "identity_id": "11111111-1111-1111-1111-111111111111", "name": "Ada",
                  "email": "a@example.com", "picture": None, "active": True, "expired": False}],
}


@pytest.fixture(params=[SsoIntrospector, AsyncSsoIntrospector], ids=["sync", "async"])
def start(request):
    def build(handler, **options):
        seen = []

        def record(http_request):
            seen.append(http_request)
            return handler(http_request)

        introspector = request.param(
            "https://accounts.test/o/auth", "drive", "secret", transport=httpx.MockTransport(record), **options
        )
        return introspector, seen

    return build


async def call(target, name, *args, **kwargs):
    result = getattr(target, name)(*args, **kwargs)
    return await result if inspect.isawaitable(result) else result


def good(request):
    token = json.loads(request.content)["token"]
    return httpx.Response(200, json=ACTIVE if token == "good-token" else {"active": False})


def test_the_header_name_and_the_token_hash():
    assert SSO_HEADER == "x-vigrise-sso"
    assert sso_token_hash("abc") == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"


async def test_an_active_token_returns_identity_session_and_accounts(start):
    introspector, seen = start(good)
    answer = await call(introspector, "introspect", "good-token")
    assert answer.active is True
    assert (answer.identity.id, answer.identity.email, answer.identity.email_verified) == (
        "11111111-1111-1111-1111-111111111111", "a@example.com", True)
    assert answer.identity.picture is None
    assert (answer.session.id, answer.session.aal) == ("sid-1", "aal1")
    assert answer.session.expires_at.isoformat() == "2026-10-12T10:00:00+00:00"
    assert (answer.accounts[0].session_id, answer.accounts[0].active, answer.accounts[0].expired) == ("sid-1", True, False)
    request = seen[-1]
    assert (request.method, request.url.path) == ("POST", "/o/auth/sessions/sso/introspect")
    assert request.headers["authorization"] == "Basic " + base64.b64encode(b"drive:secret").decode()


async def test_an_unknown_token_is_inactive(start):
    introspector, _ = start(good)
    answer = await call(introspector, "introspect", "other")
    assert answer.active is False and answer.identity is None and answer.accounts == []


async def test_introspect_active_refuses_an_inactive_token(start):
    introspector, _ = start(good)
    with pytest.raises(InactiveSessionError):
        await call(introspector, "introspect_active", "other")
    answer = await call(introspector, "introspect_active", "good-token")
    assert answer.identity.id == "11111111-1111-1111-1111-111111111111" and answer.session.id == "sid-1"


async def test_answers_are_cached_per_token_until_invalidated(start):
    introspector, seen = start(good)
    await call(introspector, "introspect", "good-token")
    await call(introspector, "introspect", "good-token")
    assert len(seen) == 1
    introspector.invalidate_session("sid-1")
    await call(introspector, "introspect", "good-token")
    assert len(seen) == 2
    introspector.invalidate_token(sso_token_hash("good-token"))
    await call(introspector, "introspect", "good-token")
    assert len(seen) == 3


async def test_cache_entries_expire_after_the_ttl(start):
    import asyncio

    introspector, seen = start(good, cache_ttl=0.03)
    await call(introspector, "introspect", "good-token")
    await asyncio.sleep(0.06)
    await call(introspector, "introspect", "good-token")
    assert len(seen) == 2


async def test_client_errors_are_misconfiguration_and_server_errors_are_unavailability(start):
    introspector, seen = start(lambda r: httpx.Response(403, json={"error": "x"}))
    for _ in range(2):
        with pytest.raises(AccountsMisconfiguredError):
            await call(introspector, "introspect", "good-token")
    assert len(seen) == 2

    introspector, _ = start(lambda r: httpx.Response(401, json={"error": "invalid_client"}))
    with pytest.raises(AccountsMisconfiguredError) as raised:
        await call(introspector, "introspect", "t")
    assert raised.value.status == 401

    introspector, _ = start(lambda r: httpx.Response(429, json={}))
    with pytest.raises(AccountsUnavailableError):
        await call(introspector, "introspect", "t")

    introspector, _ = start(lambda r: httpx.Response(502, json={}))
    with pytest.raises(AccountsUnavailableError):
        await call(introspector, "introspect", "t")
    assert introspector.unavailable_count == 1


async def test_an_unreachable_accounts_is_unavailable(start):
    def refuse(request):
        raise httpx.ConnectError("refused")

    introspector, _ = start(refuse)
    with pytest.raises(AccountsUnavailableError):
        await call(introspector, "introspect", "t")
    assert introspector.unavailable_count == 1


async def test_an_unreadable_answer_is_misconfiguration(start):
    introspector, _ = start(lambda r: httpx.Response(200, content=b"<html>"))
    with pytest.raises(AccountsMisconfiguredError):
        await call(introspector, "introspect", "t")


async def test_callers_cannot_change_what_is_cached(start):
    introspector, _ = start(good)
    first = await call(introspector, "introspect", "good-token")
    first.accounts.clear()
    second = await call(introspector, "introspect", "good-token")
    assert len(second.accounts) == 1

import httpx
import pytest

from vigrise.accounts import Jwks, JwksError

from .signer import TestSigner


def remote(handler, **options):
    seen = []

    def record(request):
        seen.append(request)
        return handler(request)

    transport = httpx.MockTransport(record)
    return Jwks.remote("https://accounts.test/jwks", transport=transport, async_transport=transport, **options), seen


def test_a_fixed_set_resolves_keys_by_kid():
    signer = TestSigner("a")
    jwks = Jwks.from_set(signer.jwk_set)
    assert jwks.key({"alg": "RS256", "kid": "a"}) is not None
    with pytest.raises(JwksError):
        jwks.key({"alg": "RS256", "kid": "other"})


def test_a_token_without_kid_uses_the_only_key():
    assert Jwks.from_set(TestSigner("a").jwk_set).key({"alg": "RS256"}) is not None


def test_a_remote_set_is_fetched_once_and_reused():
    signer = TestSigner("a")
    jwks, seen = remote(lambda r: httpx.Response(200, json=signer.jwk_set))
    jwks.key({"kid": "a"})
    jwks.key({"kid": "a"})
    assert len(seen) == 1 and seen[0].url.path == "/jwks"


def test_an_unknown_kid_refetches_once_the_cooldown_has_passed():
    old, new = TestSigner("old"), TestSigner("new")
    current = [old]
    jwks, seen = remote(lambda r: httpx.Response(200, json=current[0].jwk_set), refetch_cooldown=0)
    jwks.key({"kid": "old"})
    current[0] = new
    assert jwks.key({"kid": "new"}) is not None
    assert len(seen) == 2


def test_an_unknown_kid_does_not_refetch_inside_the_cooldown():
    jwks, seen = remote(lambda r: httpx.Response(200, json=TestSigner("old").jwk_set))
    jwks.key({"kid": "old"})
    with pytest.raises(JwksError):
        jwks.key({"kid": "unknown"})
    assert len(seen) == 1


def test_a_failing_endpoint_is_a_jwks_error():
    jwks, _ = remote(lambda r: httpx.Response(500))
    with pytest.raises(JwksError):
        jwks.key({"kid": "a"})


async def test_the_async_fetch_shares_the_cache():
    old, new = TestSigner("old"), TestSigner("new")
    current = [old]
    jwks, seen = remote(lambda r: httpx.Response(200, json=current[0].jwk_set), refetch_cooldown=0)
    assert await jwks.akey({"kid": "old"}) is not None
    assert await jwks.akey({"kid": "old"}) is not None
    assert len(seen) == 1
    current[0] = new
    assert await jwks.akey({"kid": "new"}) is not None
    assert len(seen) == 2

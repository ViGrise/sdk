import inspect
import re
import time
from urllib.parse import parse_qs, urlsplit

import httpx
import pytest

from vigrise.accounts import (
    AsyncOAuth2Client,
    AuthorizationRequest,
    IdTokenVerifier,
    InvalidTokenError,
    Jwks,
    OAuth2Client,
    Pkce,
    TokenExchangeError,
    end_session_url,
    random_state,
)

from .signer import TestSigner

ISS = "https://accounts.vigrise.com"


def test_pkce_challenge_matches_the_rfc_7636_vector():
    assert Pkce.from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk").challenge == "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"


def test_generated_verifiers_and_states_are_unique_and_url_safe():
    a, b = Pkce.generate(), Pkce.generate()
    assert a.verifier != b.verifier
    assert re.fullmatch(r"[A-Za-z0-9_-]{43}", a.verifier)
    assert random_state() != random_state()


def test_the_authorization_url_carries_pkce_state_and_options():
    url = AuthorizationRequest(
        authorization_endpoint="https://accounts.vigrise.com/o/oauth2/auth",
        client_id="vigrise-platform-console",
        redirect_uri="https://console.vigrise.com/callback",
        scope="openid email",
        state="st",
        code_challenge="ch",
        prompt="select_account",
        max_age=43200,
    ).url()
    q = {name: values[0] for name, values in parse_qs(urlsplit(url).query).items()}
    assert (q["response_type"], q["code_challenge"], q["code_challenge_method"], q["state"]) == ("code", "ch", "S256", "st")
    assert (q["prompt"], q["max_age"], q["scope"]) == ("select_account", "43200", "openid email")
    assert "login_hint" not in q


def test_the_end_session_url_carries_the_hint_and_redirect():
    q = {n: v[0] for n, v in parse_qs(urlsplit(end_session_url(
        "https://accounts.vigrise.com/o/oauth2/sessions/logout", "app", "https://app.example/bye", id_token_hint="tok", state="s")).query).items()}
    assert q == {"client_id": "app", "post_logout_redirect_uri": "https://app.example/bye", "id_token_hint": "tok", "state": "s"}
    assert "id_token_hint" not in urlsplit(end_session_url("https://a.test/logout", "app", "https://app.example/bye")).query


@pytest.fixture(params=[OAuth2Client, AsyncOAuth2Client], ids=["sync", "async"])
def make_client(request):
    def build(handler, **options):
        seen = []

        def record(http_request):
            seen.append(http_request)
            return handler(http_request)

        client = request.param(
            "https://accounts.test/token", "app", transport=httpx.MockTransport(record), **options
        )
        return client, seen

    return build


async def call(client, name, *args, **kwargs):
    result = getattr(client, name)(*args, **kwargs)
    return await result if inspect.isawaitable(result) else result


async def test_a_confidential_client_posts_the_verifier_and_credentials(make_client):
    client, seen = make_client(
        lambda r: httpx.Response(200, json={"access_token": "at", "id_token": "it", "refresh_token": "rt", "expires_in": 3600}),
        client_secret="shh",
    )
    tokens = await call(client, "exchange_code", "code-1", "verifier-1", "https://app.example/cb")
    assert (tokens.access_token, tokens.id_token, tokens.refresh_token, tokens.expires_in) == ("at", "it", "rt", 3600)
    request = seen[-1]
    assert (request.method, request.url.path) == ("POST", "/token")
    assert request.headers["content-type"] == "application/x-www-form-urlencoded"
    assert {n: v[0] for n, v in parse_qs(request.content.decode()).items()} == {
        "grant_type": "authorization_code", "code": "code-1", "redirect_uri": "https://app.example/cb",
        "client_id": "app", "code_verifier": "verifier-1", "client_secret": "shh",
    }


async def test_a_public_client_sends_no_secret_and_uses_its_default_redirect(make_client):
    client, seen = make_client(lambda r: httpx.Response(200, json={"access_token": "at"}), redirect_uri="http://127.0.0.1/cb")
    tokens = await call(client, "exchange_code", "c", "v")
    assert (tokens.access_token, tokens.id_token) == ("at", None)
    form = parse_qs(seen[-1].content.decode())
    assert "client_secret" not in form and form["redirect_uri"] == ["http://127.0.0.1/cb"]


async def test_a_refused_exchange_carries_status_and_body(make_client):
    client, _ = make_client(lambda r: httpx.Response(400, json={"error": "invalid_grant"}), client_secret="s")
    with pytest.raises(TokenExchangeError) as raised:
        await call(client, "exchange_code", "c", "v", "https://app.example/cb")
    assert "400" in str(raised.value) and "invalid_grant" in str(raised.value)


async def test_an_unreachable_token_endpoint_is_a_token_exchange_error(make_client):
    def refuse(request):
        raise httpx.ConnectError("refused")

    client, _ = make_client(refuse)
    with pytest.raises(TokenExchangeError):
        await call(client, "exchange_code", "c", "v", "https://app.example/cb")


SIGNER = TestSigner()


def id_claims(**extra):
    now = int(time.time())
    return {"iss": ISS, "aud": "app", "sub": "user-1", "sid": "sid-1", "nonce": "n-1", "iat": now, "exp": now + 300,
            "auth_time": now, **extra}


@pytest.fixture(params=["verify", "averify"])
def verify_name(request):
    return request.param


def id_verifier():
    return IdTokenVerifier(ISS, "app", Jwks.from_set(SIGNER.jwk_set))


async def test_id_tokens_are_verified_for_issuer_audience_and_nonce(verify_name):
    verifier = id_verifier()

    async def run(token, **kwargs):
        result = getattr(verifier, verify_name)(token, **kwargs)
        return await result if inspect.isawaitable(result) else result

    result = await run(SIGNER.sign(id_claims()), nonce="n-1")
    assert (result.sub, result.sid, result.nonce) == ("user-1", "sid-1", "n-1")
    with pytest.raises(InvalidTokenError, match="nonce"):
        await run(SIGNER.sign(id_claims()), nonce="other")
    for claims in (id_claims(iss="https://evil.example"), id_claims(aud="someone-else"), id_claims(exp=int(time.time()) - 600)):
        with pytest.raises(InvalidTokenError):
            await run(SIGNER.sign(claims))
    assert (await run(SIGNER.sign(id_claims()))).sub == "user-1"


async def test_verify_for_audience_checks_the_audience_it_is_given():
    verifier = id_verifier()
    token = SIGNER.sign(id_claims(aud="desktop"))
    assert (verifier.verify_for_audience(token, "desktop")).sub == "user-1"
    assert (await verifier.averify_for_audience(token, "desktop")).sub == "user-1"
    with pytest.raises(InvalidTokenError):
        verifier.verify(token)


def test_id_tokens_tolerate_sixty_seconds_of_clock_skew():
    verifier = id_verifier()
    assert verifier.verify(SIGNER.sign(id_claims(exp=int(time.time()) - 10))).sub == "user-1"
    with pytest.raises(InvalidTokenError):
        verifier.verify(SIGNER.sign(id_claims(exp=int(time.time()) - 120)))

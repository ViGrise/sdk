import base64
import hashlib
import secrets
from dataclasses import dataclass
from typing import Optional
from urllib.parse import urlencode, urlsplit, urlunsplit

import httpx

from .errors import InvalidTokenError, TokenExchangeError
from .jwks import Jwks
from .dto import IdTokenClaims, TokenResponse
from .verify import decode, unverified_header

REQUEST_TIMEOUT = 5.0
# Clock skew an ID token's expiry tolerates.
ID_TOKEN_LEEWAY_SECONDS = 60


def _random_urlsafe(size: int) -> str:
  return base64.urlsafe_b64encode(secrets.token_bytes(size)).decode().rstrip("=")


@dataclass(frozen=True)
class Pkce:
  """A PKCE verifier and its S256 challenge."""

  # Secret kept by the app and sent with the code exchange.
  verifier: str
  # Challenge sent with the authorization request.
  challenge: str

  @classmethod
  def generate(cls) -> "Pkce":
    """Generates a 43 character verifier from 32 random bytes."""
    return cls.from_verifier(_random_urlsafe(32))

  @classmethod
  def from_verifier(cls, verifier: str) -> "Pkce":
    """Derives the challenge of an existing verifier."""
    digest = hashlib.sha256(verifier.encode()).digest()
    return cls(verifier, base64.urlsafe_b64encode(digest).decode().rstrip("="))


def random_state() -> str:
  """A random `state` (or `nonce`) value."""
  return _random_urlsafe(32)


def _with_query(url: str, pairs) -> str:
  parts = urlsplit(url)
  query = "&".join(filter(None, [parts.query, urlencode(pairs)]))
  return urlunsplit((parts.scheme, parts.netloc, parts.path, query, parts.fragment))


@dataclass(frozen=True)
class AuthorizationRequest:
  """An authorization request for the code flow with PKCE."""

  # The `authorization_endpoint` of the discovery document.
  authorization_endpoint: str
  client_id: str
  # Registered redirect URI.
  redirect_uri: str
  # Space separated scopes.
  scope: str
  # Anti-CSRF value echoed back on the redirect.
  state: str
  # S256 challenge of the PKCE verifier.
  code_challenge: str
  # `prompt` parameter such as `none` or `select_account`.
  prompt: Optional[str] = None
  # `max_age` in seconds.
  max_age: Optional[int] = None
  login_hint: Optional[str] = None

  def url(self) -> str:
    """Builds the URL to send the browser to."""
    pairs = [
      ("response_type", "code"),
      ("client_id", self.client_id),
      ("redirect_uri", self.redirect_uri),
      ("scope", self.scope),
      ("state", self.state),
      ("code_challenge", self.code_challenge),
      ("code_challenge_method", "S256"),
    ]
    if self.prompt is not None:
      pairs.append(("prompt", self.prompt))
    if self.max_age is not None:
      pairs.append(("max_age", str(self.max_age)))
    if self.login_hint is not None:
      pairs.append(("login_hint", self.login_hint))
    return _with_query(self.authorization_endpoint, pairs)


def end_session_url(
  end_session_endpoint: str,
  client_id: str,
  post_logout_redirect_uri: str,
  *,
  id_token_hint: Optional[str] = None,
  state: Optional[str] = None,
) -> str:
  """Builds the RP-initiated logout URL."""
  pairs = [("client_id", client_id), ("post_logout_redirect_uri", post_logout_redirect_uri)]
  if id_token_hint is not None:
    pairs.append(("id_token_hint", id_token_hint))
  if state is not None:
    pairs.append(("state", state))
  return _with_query(end_session_endpoint, pairs)


class _OAuth2Client:
  def __init__(
    self,
    token_endpoint: str,
    client_id: str,
    client_secret: Optional[str],
    redirect_uri: Optional[str],
    timeout: float,
  ):
    self._token_endpoint = token_endpoint
    self._client_id = client_id
    self._client_secret = client_secret
    self._redirect_uri = redirect_uri
    self._timeout = timeout

  def _build(self, http, code: str, verifier: str, redirect_uri: Optional[str]) -> httpx.Request:
    redirect = redirect_uri or self._redirect_uri
    if redirect is None:
      raise TokenExchangeError("authorization code exchange failed: no redirect URI")
    form = {
      "grant_type": "authorization_code",
      "code": code,
      "redirect_uri": redirect,
      "client_id": self._client_id,
      "code_verifier": verifier,
    }
    if self._client_secret is not None:
      form["client_secret"] = self._client_secret
    return http.build_request(
      "POST",
      self._token_endpoint,
      data=form,
      headers={"accept": "application/json"},
      timeout=self._timeout,
    )

  @staticmethod
  def _read(response: httpx.Response) -> TokenResponse:
    if not response.is_success:
      raise TokenExchangeError(
        f"authorization code exchange failed: status {response.status_code}: {response.text}"
      )
    try:
      data = response.json()
      return TokenResponse(
        data["access_token"],
        data.get("id_token"),
        data.get("refresh_token"),
        data.get("expires_in"),
      )
    except (ValueError, KeyError, TypeError) as error:
      raise TokenExchangeError(f"authorization code exchange failed: {error}") from error


class OAuth2Client(_OAuth2Client):
  """Exchanges authorization codes at the token endpoint. Without a `client_secret` it is a public client
  (CLI, desktop app) that proves possession of the PKCE verifier."""

  def __init__(
    self,
    token_endpoint: str,
    client_id: str,
    client_secret: Optional[str] = None,
    redirect_uri: Optional[str] = None,
    *,
    timeout: float = REQUEST_TIMEOUT,
    transport: Optional[httpx.BaseTransport] = None,
  ):
    super().__init__(token_endpoint, client_id, client_secret, redirect_uri, timeout)
    self._http = httpx.Client(transport=transport)

  def close(self) -> None:
    self._http.close()

  def exchange_code(
    self, code: str, verifier: str, redirect_uri: Optional[str] = None
  ) -> TokenResponse:
    """Exchanges `code` with the PKCE `verifier` of the authorization request."""
    try:
      response = self._http.send(self._build(self._http, code, verifier, redirect_uri))
    except httpx.HTTPError as error:
      raise TokenExchangeError(f"authorization code exchange failed: {error}") from error
    return self._read(response)


class AsyncOAuth2Client(_OAuth2Client):
  """The async counterpart of `OAuth2Client`."""

  def __init__(
    self,
    token_endpoint: str,
    client_id: str,
    client_secret: Optional[str] = None,
    redirect_uri: Optional[str] = None,
    *,
    timeout: float = REQUEST_TIMEOUT,
    transport: Optional[httpx.AsyncBaseTransport] = None,
  ):
    super().__init__(token_endpoint, client_id, client_secret, redirect_uri, timeout)
    self._http = httpx.AsyncClient(transport=transport)

  async def aclose(self) -> None:
    await self._http.aclose()

  async def exchange_code(
    self, code: str, verifier: str, redirect_uri: Optional[str] = None
  ) -> TokenResponse:
    """Exchanges `code` with the PKCE `verifier` of the authorization request."""
    try:
      response = await self._http.send(self._build(self._http, code, verifier, redirect_uri))
    except httpx.HTTPError as error:
      raise TokenExchangeError(f"authorization code exchange failed: {error}") from error
    return self._read(response)


class IdTokenVerifier:
  """Verifies ID tokens issued to one client."""

  def __init__(self, issuer: str, client_id: str, jwks: Jwks):
    self._issuer = issuer
    self._client_id = client_id
    self._jwks = jwks

  def verify(self, id_token: str, *, nonce: Optional[str] = None) -> IdTokenClaims:
    """Verifies signature, `iss`, `aud`, expiry and, when given, the nonce."""
    return self._with_nonce(self.verify_for_audience(id_token, self._client_id), nonce)

  async def averify(self, id_token: str, *, nonce: Optional[str] = None) -> IdTokenClaims:
    """Like `verify`, fetching the signing keys with an async client."""
    return self._with_nonce(await self.averify_for_audience(id_token, self._client_id), nonce)

  def verify_for_audience(self, id_token: str, audience: str) -> IdTokenClaims:
    """Verifies signature, `iss`, expiry and that the token is addressed to `audience`, for backends that
    accept ID tokens minted for another client such as a desktop app."""
    header = unverified_header(id_token, "ID token")
    return self._claims(id_token, self._jwks.key(header), audience)

  async def averify_for_audience(self, id_token: str, audience: str) -> IdTokenClaims:
    header = unverified_header(id_token, "ID token")
    return self._claims(id_token, await self._jwks.akey(header), audience)

  def _claims(self, id_token: str, key, audience: str) -> IdTokenClaims:
    claims = decode(
      id_token,
      key,
      "ID token",
      self._issuer,
      audience,
      ["iss", "aud", "exp", "sub"],
      ID_TOKEN_LEEWAY_SECONDS,
    )
    return IdTokenClaims(
      claims["sub"], claims.get("sid"), claims.get("auth_time"), claims.get("nonce")
    )

  @staticmethod
  def _with_nonce(claims: IdTokenClaims, nonce: Optional[str]) -> IdTokenClaims:
    if nonce is not None and claims.nonce != nonce:
      raise InvalidTokenError("invalid ID token: nonce mismatch")
    return claims

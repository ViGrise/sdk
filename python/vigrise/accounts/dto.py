from dataclasses import dataclass
from typing import Optional


@dataclass(frozen=True)
class LogoutResult:
  """The claims of a verified `logout_token`, and whether its `jti` was seen before."""

  # `fresh` for the first delivery: end the session. `replayed` for a `jti` already processed: answer 200 and do nothing.
  outcome: str
  jti: str
  # Client-facing subject of the person.
  sub: Optional[str] = None
  # Accounts session id that ended.
  sid: Optional[str] = None


@dataclass(frozen=True)
class TokenResponse:
  """The token endpoint answer."""

  access_token: str
  # Present for the `openid` scope.
  id_token: Optional[str] = None
  refresh_token: Optional[str] = None
  # Access token lifetime in seconds.
  expires_in: Optional[int] = None


@dataclass(frozen=True)
class IdTokenClaims:
  """Claims of a verified ID token."""

  # Client-facing subject.
  sub: str
  # Accounts session id.
  sid: Optional[str] = None
  # When the person authenticated, in seconds since the epoch.
  auth_time: Optional[int] = None
  # Nonce echoed from the authorization request.
  nonce: Optional[str] = None

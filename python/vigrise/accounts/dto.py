from dataclasses import dataclass, field
from datetime import datetime
from typing import Any, Dict, List, Optional

from ..auth.sts import parse_timestamp


@dataclass(frozen=True)
class SsoIdentity:
    """The person behind an SSO session."""

    # Accounts identity id, the `sub` apps key their users by.
    id: str
    email: Optional[str] = None
    name: Optional[str] = None
    picture: Optional[str] = None
    email_verified: bool = False


@dataclass(frozen=True)
class SsoSession:
    """An Accounts session."""

    # Session id, the `sso_sid` of app sessions and the `sid` of logout tokens.
    id: str
    authenticated_at: datetime
    # Authenticator assurance level.
    aal: str
    expires_at: datetime


@dataclass(frozen=True)
class SsoAccount:
    """An account in the browser's group, for the account menu."""

    # Session id to switch to.
    session_id: str
    identity_id: str
    active: bool
    name: Optional[str] = None
    email: Optional[str] = None
    picture: Optional[str] = None
    # Whether the session expired and needs re-authentication.
    expired: bool = False


@dataclass(frozen=True)
class Introspection:
    """The answer of the introspection endpoint."""

    # False for an unknown, revoked or ended token.
    active: bool
    identity: Optional[SsoIdentity] = None
    session: Optional[SsoSession] = None
    accounts: List[SsoAccount] = field(default_factory=list)

    @classmethod
    def from_dict(cls, data: Dict[str, Any]) -> "Introspection":
        identity = data.get("identity")
        session = data.get("session")
        return cls(
            active=bool(data.get("active")),
            identity=SsoIdentity(
                id=identity["id"],
                email=identity.get("email"),
                name=identity.get("name"),
                picture=identity.get("picture"),
                email_verified=bool(identity.get("email_verified", False)),
            ) if identity else None,
            session=SsoSession(
                id=session["id"],
                authenticated_at=parse_timestamp(session["authenticated_at"]),
                aal=session["aal"],
                expires_at=parse_timestamp(session["expires_at"]),
            ) if session else None,
            accounts=[
                SsoAccount(
                    session_id=item["session_id"],
                    identity_id=item["identity_id"],
                    active=bool(item["active"]),
                    name=item.get("name"),
                    email=item.get("email"),
                    picture=item.get("picture"),
                    expired=bool(item.get("expired", False)),
                )
                for item in data.get("accounts") or []
            ],
        )


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

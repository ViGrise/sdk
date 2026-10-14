import threading
import time
from typing import Any, Dict

from .errors import InvalidTokenError
from .jwks import Jwks
from .dto import LogoutResult
from .verify import LEEWAY_SECONDS, decode, unverified_header

EVENT = "http://schemas.openid.net/event/backchannel-logout"
MAX_AGE_SECONDS = 300


class BackchannelLogout:
    """Verifies back-channel `logout_token`s sent by Accounts: signature against the JWKS, `iss`, `aud`, the
    logout event, the time window and `jti` replay."""

    def __init__(self, issuer: str, client_id: str, jwks: Jwks):
        self._issuer = issuer
        self._client_id = client_id
        self._jwks = jwks
        self._seen: Dict[str, int] = {}
        self._lock = threading.Lock()

    def verify(self, logout_token: str) -> LogoutResult:
        header = unverified_header(logout_token, "logout token")
        return self._check(logout_token, self._jwks.key(header))

    async def averify(self, logout_token: str) -> LogoutResult:
        """Like `verify`, fetching the signing keys with an async client."""
        header = unverified_header(logout_token, "logout token")
        return self._check(logout_token, await self._jwks.akey(header))

    def _check(self, logout_token: str, key: Any) -> LogoutResult:
        claims = decode(logout_token, key, "logout token", self._issuer, self._client_id, ["iss", "aud", "exp", "iat"], LEEWAY_SECONDS)

        def invalid(message: str) -> InvalidTokenError:
            return InvalidTokenError(f"invalid logout token: {message}")

        events = claims.get("events")
        if not isinstance(events, dict) or events.get(EVENT) is None:
            raise invalid("missing backchannel-logout event")
        if "nonce" in claims:
            raise invalid("nonce is not allowed")
        if "sid" not in claims and "sub" not in claims:
            raise invalid("neither sid nor sub")
        jti = claims.get("jti")
        if not isinstance(jti, str) or not jti:
            raise invalid("missing jti")
        now = int(time.time())
        if claims["iat"] > now + LEEWAY_SECONDS or claims["iat"] < now - MAX_AGE_SECONDS - LEEWAY_SECONDS:
            raise invalid("issued outside the accepted time window")
        with self._lock:
            self._seen = {seen: exp for seen, exp in self._seen.items() if exp >= now}
            replayed = jti in self._seen
            self._seen[jti] = claims["exp"]
        return LogoutResult("replayed" if replayed else "fresh", jti, claims.get("sub"), claims.get("sid"))

import base64
import threading
import time
from typing import Dict, Optional, Tuple

import httpx

from .._http import Request
from ._errors import AccountsMisconfiguredError, AccountsUnavailableError, InactiveSessionError
from ._hash import sso_token_hash
from ._models import Introspection

MAX_CACHE_ENTRIES = 10_000
REQUEST_TIMEOUT = 5.0


class _Introspector:
    """The cache, the request and the reading of answers that the sync and async introspectors share."""

    def __init__(self, internal_url: str, client_id: str, client_secret: str, cache_ttl: float, timeout: float):
        self._url = internal_url.rstrip("/") + "/sessions/sso/introspect"
        self._authorization = "Basic " + base64.b64encode(f"{client_id}:{client_secret}".encode()).decode()
        self._cache_ttl = cache_ttl
        self._timeout = timeout
        self._cache: Dict[str, Tuple[float, Introspection]] = {}
        self._lock = threading.Lock()
        self._unavailable = 0

    @property
    def unavailable_count(self) -> int:
        """How many introspections failed because Accounts was unreachable or answered with a server error. Export it as a metric to alert on."""
        return self._unavailable

    def _cached(self, key: str) -> Optional[Introspection]:
        with self._lock:
            entry = self._cache.get(key)
        if entry is not None and time.monotonic() - entry[0] < self._cache_ttl:
            return entry[1]
        return None

    def _store(self, key: str, answer: Introspection) -> None:
        with self._lock:
            if len(self._cache) >= MAX_CACHE_ENTRIES:
                now = time.monotonic()
                self._cache = {k: v for k, v in self._cache.items() if now - v[0] < self._cache_ttl}
            self._cache[key] = (time.monotonic(), answer)

    def invalidate_session(self, session_id: str) -> None:
        """Drops every cached answer of an Accounts session. The cache lives in this process, so other instances keep answering from their own cache until the entries expire."""
        with self._lock:
            self._cache = {k: v for k, v in self._cache.items() if not (v[1].session and v[1].session.id == session_id)}

    def invalidate_token(self, token_hash: str) -> None:
        """Drops the cached answer of one token, given its `sso_token_hash`."""
        with self._lock:
            self._cache.pop(token_hash, None)

    def _build(self, http: "httpx.Client | httpx.AsyncClient", token: str) -> httpx.Request:
        return http.build_request("POST", self._url, json={"token": token}, headers={"authorization": self._authorization}, timeout=self._timeout)

    def _unreachable(self, error: Exception) -> AccountsUnavailableError:
        self._unavailable += 1
        return AccountsUnavailableError(f"ViGrise Accounts is unavailable: {error}")

    def _read(self, response: httpx.Response) -> Introspection:
        status = response.status_code
        if status == 401:
            raise AccountsMisconfiguredError("ViGrise Accounts rejected the client credentials", status)
        if status == 403:
            raise AccountsMisconfiguredError("the client is not allowed to introspect SSO tokens", status)
        if status == 429:
            raise AccountsUnavailableError("the SSO introspection rate limit is exhausted", status)
        if status >= 500:
            self._unavailable += 1
            raise AccountsUnavailableError(f"ViGrise Accounts is unavailable: status {status}", status)
        if status != 200:
            raise AccountsMisconfiguredError(f"unexpected answer from ViGrise Accounts: status {status}", status)
        try:
            return Introspection.from_dict(response.json())
        except (ValueError, KeyError, TypeError, AttributeError) as error:
            raise AccountsMisconfiguredError(f"unexpected answer from ViGrise Accounts: {error}", status) from error


def require_active(answer: Introspection) -> Introspection:
    if not answer.active or answer.identity is None or answer.session is None:
        raise InactiveSessionError()
    return answer


class SsoIntrospector(_Introspector):
    """Introspects `vigrise_sso` tokens against Accounts and caches the answers by token hash."""

    def __init__(self, internal_url: str, client_id: str, client_secret: str, *, cache_ttl: float = 30.0,
                 timeout: float = REQUEST_TIMEOUT, transport: Optional[httpx.BaseTransport] = None):
        """`internal_url` is the base URL of the Accounts identity paths reachable from the app server,
        for example `http://accounts-backend:8080/o/auth`."""
        super().__init__(internal_url, client_id, client_secret, cache_ttl, timeout)
        self._http = httpx.Client(transport=transport)

    def close(self) -> None:
        self._http.close()

    def introspect(self, token: str) -> Introspection:
        """Introspects a `vigrise_sso` value. Failures are never cached."""
        key = sso_token_hash(token)
        cached = self._cached(key)
        if cached is not None:
            return cached
        try:
            response = self._http.send(self._build(self._http, token))
        except httpx.HTTPError as error:
            raise self._unreachable(error) from error
        answer = self._read(response)
        self._store(key, answer)
        return answer

    def introspect_active(self, token: str) -> Introspection:
        """Like `introspect`, and raises `InactiveSessionError` for a token that is not active."""
        return require_active(self.introspect(token))


class AsyncSsoIntrospector(_Introspector):
    """The async counterpart of `SsoIntrospector`."""

    def __init__(self, internal_url: str, client_id: str, client_secret: str, *, cache_ttl: float = 30.0,
                 timeout: float = REQUEST_TIMEOUT, transport: Optional[httpx.AsyncBaseTransport] = None):
        super().__init__(internal_url, client_id, client_secret, cache_ttl, timeout)
        self._http = httpx.AsyncClient(transport=transport)

    async def aclose(self) -> None:
        await self._http.aclose()

    async def introspect(self, token: str) -> Introspection:
        """Introspects a `vigrise_sso` value. Failures are never cached."""
        key = sso_token_hash(token)
        cached = self._cached(key)
        if cached is not None:
            return cached
        try:
            response = await self._http.send(self._build(self._http, token))
        except httpx.HTTPError as error:
            raise self._unreachable(error) from error
        answer = self._read(response)
        self._store(key, answer)
        return answer

    async def introspect_active(self, token: str) -> Introspection:
        """Like `introspect`, and raises `InactiveSessionError` for a token that is not active."""
        return require_active(await self.introspect(token))

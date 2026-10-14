import threading
import time
from typing import Any, Dict, Mapping, Optional

import httpx
import jwt

from ._errors import JwksError

CACHE_TTL = 600.0
REFETCH_COOLDOWN = 30.0
FETCH_TIMEOUT = 5.0


class Jwks:
    """Signing keys of ViGrise Accounts, either fixed or fetched from a JWKS URL and cached for ten minutes.
    A token that names an unknown key id refetches the set once the cooldown has passed."""

    def __init__(self, url: Optional[str], fixed: Optional[Mapping[str, Any]], refetch_cooldown: float,
                 transport: Optional[httpx.BaseTransport], async_transport: Optional[httpx.AsyncBaseTransport]):
        self._url = url
        self._set = jwt.PyJWKSet.from_dict(dict(fixed)) if fixed is not None else None
        self._fetched_at = 0.0
        self._cooldown = refetch_cooldown
        self._transport = transport
        self._async_transport = async_transport
        self._lock = threading.Lock()

    @classmethod
    def from_set(cls, key_set: Mapping[str, Any]) -> "Jwks":
        """Uses a fixed key set."""
        return cls(None, key_set, 0.0, None, None)

    @classmethod
    def remote(cls, url: str, *, refetch_cooldown: float = REFETCH_COOLDOWN,
               transport: Optional[httpx.BaseTransport] = None,
               async_transport: Optional[httpx.AsyncBaseTransport] = None) -> "Jwks":
        """Fetches the key set from `url`."""
        return cls(url, None, refetch_cooldown, transport, async_transport)

    def _cached(self, header: Dict[str, Any]):
        """The key from the cached set, or what to do next: ("fetch") or ("fail", error)."""
        age = time.monotonic() - self._fetched_at
        if self._set is not None and age < CACHE_TTL:
            try:
                return "key", self._find(self._set, header)
            except JwksError as error:
                if age < self._cooldown:
                    return "fail", error
        elif self._set is not None and age < self._cooldown:
            try:
                return "key", self._find(self._set, header)
            except JwksError as error:
                return "fail", error
        return "fetch", None

    def key(self, header: Dict[str, Any]):
        """The key a token with this header is signed with."""
        if self._url is None:
            return self._find(self._set, header)
        step, value = self._cached(header)
        if step == "key":
            return value
        if step == "fail":
            raise value
        try:
            with httpx.Client(transport=self._transport, timeout=FETCH_TIMEOUT) as http:
                response = http.get(self._url)
            return self._adopt(response, header)
        except httpx.HTTPError as error:
            raise JwksError(str(error)) from error

    async def akey(self, header: Dict[str, Any]):
        """Like `key`, fetching the set with an async client."""
        if self._url is None:
            return self._find(self._set, header)
        step, value = self._cached(header)
        if step == "key":
            return value
        if step == "fail":
            raise value
        try:
            async with httpx.AsyncClient(transport=self._async_transport, timeout=FETCH_TIMEOUT) as http:
                response = await http.get(self._url)
            return self._adopt(response, header)
        except httpx.HTTPError as error:
            raise JwksError(str(error)) from error

    def _adopt(self, response: httpx.Response, header: Dict[str, Any]):
        try:
            response.raise_for_status()
            fetched = jwt.PyJWKSet.from_dict(response.json())
        except (httpx.HTTPError, ValueError, jwt.PyJWTError, KeyError, TypeError) as error:
            raise JwksError(f"the key set could not be loaded: {error}") from error
        with self._lock:
            self._set = fetched
            self._fetched_at = time.monotonic()
        return self._find(fetched, header)

    @staticmethod
    def _find(key_set, header: Dict[str, Any]):
        kid = header.get("kid")
        keys = key_set.keys
        if kid is not None:
            matches = [key for key in keys if key.key_id == kid]
        else:
            matches = keys if len(keys) == 1 else []
        if not matches:
            raise JwksError(f"no signing key for kid {kid!r}")
        return matches[0].key

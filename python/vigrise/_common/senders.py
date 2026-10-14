"""Sending of signed requests, shared by the service clients. The sync and async senders differ only in how they send."""

from dataclasses import dataclass
from typing import Callable, Optional

import httpx

from . import http as _http
from ..auth.credentials import CredentialsSource


@dataclass(frozen=True)
class Failures:
    """The errors a service client raises."""

    signing: Callable[[str], Exception]
    transport: Callable[[str], Exception]
    response: Callable[[int, bytes], Exception]


class _Sender:
    def _refused(self, status: int) -> None:
        """Tells a credentials provider that the service refused its credentials, for example a revoked session."""
        if status in (401, 403):
            invalidate = getattr(self.credentials, "invalidate", None)
            if callable(invalidate):
                invalidate()

    def __init__(self, service: str, endpoint: str, credentials: Optional[CredentialsSource], timeout: Optional[float], failures: Failures):
        self.service = service
        self.endpoint = endpoint.rstrip("/")
        self.credentials = credentials
        self.timeout = timeout
        self.failures = failures

    def _build(self, http, request: _http.Request) -> httpx.Request:
        try:
            url, headers = _http.prepare(request, self.endpoint, self.service, self.credentials)
        except Exception as error:
            raise self.failures.signing(str(error)) from error
        content = request.stream if request.stream is not None else request.content
        return http.build_request(request.method, url, headers=headers, content=content, timeout=self.timeout)


class SyncSender(_Sender):
    def __init__(self, service, endpoint, credentials, timeout, failures, transport: Optional[httpx.BaseTransport] = None):
        super().__init__(service, endpoint, credentials, timeout, failures)
        self.http = httpx.Client(transport=transport)

    def close(self) -> None:
        self.http.close()

    def send(self, request: _http.Request, stream: bool = False) -> httpx.Response:
        """The response of a request that succeeded; with `stream`, its body is not read yet."""
        built = self._build(self.http, request)
        try:
            response = self.http.send(built, stream=stream)
        except httpx.HTTPError as error:
            raise self.failures.transport(str(error)) from error
        if not response.is_success:
            body = response.read()
            response.close()
            self._refused(response.status_code)
            raise self.failures.response(response.status_code, body)
        return response


class AsyncSender(_Sender):
    def __init__(self, service, endpoint, credentials, timeout, failures, transport: Optional[httpx.AsyncBaseTransport] = None):
        super().__init__(service, endpoint, credentials, timeout, failures)
        self.http = httpx.AsyncClient(transport=transport)

    async def aclose(self) -> None:
        await self.http.aclose()

    async def send(self, request: _http.Request, stream: bool = False) -> httpx.Response:
        """The response of a request that succeeded; with `stream`, its body is not read yet."""
        built = self._build(self.http, request)
        try:
            response = await self.http.send(built, stream=stream)
        except httpx.HTTPError as error:
            raise self.failures.transport(str(error)) from error
        if not response.is_success:
            body = await response.aread()
            await response.aclose()
            self._refused(response.status_code)
            raise self.failures.response(response.status_code, body)
        return response

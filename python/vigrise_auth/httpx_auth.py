"""`httpx` integration: `httpx.post(url, json=..., auth=VigriseHttpxAuth("tts"))`."""

from typing import Generator, Optional

import httpx

from .credentials import Credentials, resolve_credentials
from .signer import sign_request


class VigriseHttpxAuth(httpx.Auth):
    requires_request_body = True

    def __init__(self, service: str, credentials: Optional[Credentials] = None):
        self.service = service
        self.credentials = credentials

    def auth_flow(self, request: httpx.Request) -> Generator[httpx.Request, httpx.Response, None]:
        signed = sign_request(
            request.method,
            str(request.url),
            list(request.headers.items()),
            request.content,
            self.credentials or resolve_credentials(),
            self.service,
        )
        request.headers.update(signed)
        yield request

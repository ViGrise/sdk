"""`httpx` integration: `httpx.post(url, json=..., auth=ViGriseHttpxAuth("speech"))`."""

from typing import Generator, Optional

import httpx

from .credentials import CredentialsSource, credentials_from
from .signer import sign_request


class ViGriseHttpxAuth(httpx.Auth):
    requires_request_body = True

    def __init__(self, service: str, credentials: Optional[CredentialsSource] = None):
        self.service = service
        self.credentials = credentials

    def auth_flow(self, request: httpx.Request) -> Generator[httpx.Request, httpx.Response, None]:
        signed = sign_request(
            request.method,
            str(request.url),
            request.headers.multi_items(),
            request.content,
            credentials_from(self.credentials),
            self.service,
        )
        request.headers.update(signed)
        yield request

"""`requests` integration: `requests.post(url, json=..., auth=ViGriseAuth("speech"))`."""

from typing import Optional

import requests

from .credentials import CredentialsSource, credentials_from
from .signer import sign_request


class ViGriseAuth(requests.auth.AuthBase):
    def __init__(self, service: str, credentials: Optional[CredentialsSource] = None):
        self.service = service
        self.credentials = credentials

    def __call__(self, request: requests.PreparedRequest) -> requests.PreparedRequest:
        body = request.body
        if body is not None and not isinstance(body, (str, bytes)):
            raise ValueError("Streaming request bodies cannot be signed; pass str or bytes")
        signed = sign_request(
            request.method or "GET",
            request.url or "",
            list(request.headers.items()),
            body,
            credentials_from(self.credentials),
            self.service,
        )
        request.headers.update(signed)
        return request

"""`requests` integration: `requests.post(url, json=..., auth=VigriseAuth("tts"))`."""

from typing import Optional

import requests

from .credentials import Credentials, resolve_credentials
from .signer import sign_request


class VigriseAuth(requests.auth.AuthBase):
    def __init__(self, service: str, credentials: Optional[Credentials] = None):
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
            self.credentials or resolve_credentials(),
            self.service,
        )
        request.headers.update(signed)
        return request

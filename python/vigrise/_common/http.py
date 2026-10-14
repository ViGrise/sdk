"""Request building shared by the service clients: the sync and async clients differ only in sending."""

from dataclasses import dataclass, field
from typing import AsyncIterable, Dict, Iterable, List, Optional, Tuple, Union
from urllib.parse import quote

from ..auth import UNSIGNED_PAYLOAD, sign_request
from ..auth.credentials import CredentialsSource, credentials_from


@dataclass
class Request:
    """A request before it is signed: the path is relative to the endpoint."""

    method: str
    path: str
    query: List[Tuple[str, str]] = field(default_factory=list)
    headers: Dict[str, str] = field(default_factory=dict)
    content: bytes = b""
    # A body that streams; it is signed as UNSIGNED_PAYLOAD and needs a content-length header.
    stream: Optional[Union[Iterable[bytes], AsyncIterable[bytes]]] = None


def segment(value: str) -> str:
    """A path segment with everything but unreserved characters percent-encoded."""
    return quote(value, safe="")


def query_pairs(**values: object) -> List[Tuple[str, str]]:
    """The query parameters that have a value, in the order given; booleans are `true` or `false`."""
    pairs = []
    for name, value in values.items():
        if value is None:
            continue
        pairs.append((name, ("true" if value else "false") if isinstance(value, bool) else str(getattr(value, "value", value))))
    return pairs


def encode_query(pairs: Iterable[Tuple[str, str]]) -> str:
    return "&".join(f"{quote(name, safe='')}={quote(value, safe='')}" for name, value in pairs)


def prepare(request: Request, endpoint: str, service: str, credentials: Optional[CredentialsSource]) -> Tuple[str, Dict[str, str]]:
    """The URL to send and the headers, signed for `service` with VGR4."""
    url = endpoint.rstrip("/") + request.path
    if request.query:
        url += "?" + encode_query(request.query)
    headers = dict(request.headers)
    headers.update(
        sign_request(
            request.method,
            url,
            headers,
            request.content,
            credentials_from(credentials),
            service,
            payload_hash=UNSIGNED_PAYLOAD if request.stream is not None else None,
        )
    )
    return url, headers

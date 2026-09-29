from datetime import datetime, timezone
from typing import Dict, List, Mapping, Optional, Sequence, Tuple, Union
from urllib.parse import urlsplit

from .canonical import canonical_request, sha256_hex
from .credentials import Credentials
from .signing import ALGORITHM, TIMESTAMP_FORMAT, derive_signing_key, scope, signature, string_to_sign

Headers = Union[Mapping[str, str], Sequence[Tuple[str, str]]]

_DEFAULT_PORTS = {"http": 80, "https": 443}


# Payload hash of a request whose body streams unsigned.
UNSIGNED_PAYLOAD = "UNSIGNED-PAYLOAD"

def _host(url) -> str:
    """Host as HTTP clients send it: without a default port."""
    host = url.hostname or ""
    if ":" in host:
        host = f"[{host}]"
    if url.port is not None and url.port != _DEFAULT_PORTS.get(url.scheme):
        host = f"{host}:{url.port}"
    return host


def sign_request(
    method: str,
    url: str,
    headers: Optional[Headers],
    body: Union[str, bytes, None],
    credentials: Credentials,
    service: str,
    now: Optional[datetime] = None,
    payload_hash: Optional[str] = None,
) -> Dict[str, str]:
    """Returns the authorization, x-vgr-date and x-vgr-content-sha256 headers.
    Signs host, content-type when present, and every x-vgr-* header.
    `payload_hash` replaces the body's SHA-256, for example UNSIGNED_PAYLOAD
    for a body that streams unsigned."""
    if credentials.access_key_id.startswith("VGST") and not credentials.session_token:
        raise ValueError("Temporary credentials require a session token")
    parts = urlsplit(url)
    pairs: List[Tuple[str, str]] = list(headers.items()) if isinstance(headers, Mapping) else list(headers or [])
    payload = body.encode() if isinstance(body, str) else (body or b"")
    timestamp = (now or datetime.now(timezone.utc)).astimezone(timezone.utc).strftime(TIMESTAMP_FORMAT)
    date = timestamp[:8]
    payload_hash = payload_hash or sha256_hex(payload)

    signing = [(n, v) for n, v in pairs if n.lower() not in ("x-vgr-date", "x-vgr-content-sha256")]
    if not any(n.lower() == "host" for n, _ in signing):
        signing.append(("host", _host(parts)))
    signing += [("x-vgr-date", timestamp), ("x-vgr-content-sha256", payload_hash)]
    if credentials.session_token:
        signing.append(("x-vgr-security-token", credentials.session_token))
    signed = sorted({n.lower() for n, _ in signing if n.lower() in ("host", "content-type") or n.lower().startswith("x-vgr-")})

    creq = canonical_request(method, parts.path, parts.query, signing, signed, payload_hash)
    credential_scope = scope(date, service)
    sts = string_to_sign(timestamp, credential_scope, creq)
    sig = signature(derive_signing_key(credentials.secret_access_key, date, service), sts)
    result = {
        "authorization": f"{ALGORITHM} Credential={credentials.access_key_id}/{credential_scope}, "
        f"SignedHeaders={';'.join(signed)}, Signature={sig}",
        "x-vgr-date": timestamp,
        "x-vgr-content-sha256": payload_hash,
    }
    if credentials.session_token:
        result["x-vgr-security-token"] = credentials.session_token
    return result

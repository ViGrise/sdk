"""STS `AssumeRole`: `assume_role(role_vrn, "web", endpoint=...)` returns a role session."""

import json
import re
import urllib.error
import urllib.request
from dataclasses import dataclass
from datetime import datetime
from typing import Any, Optional

from .credentials import Credentials, CredentialsSource, credentials_from
from .signer import sign_request


class StsError(Exception):
    def __init__(self, code: str, message: str):
        super().__init__(f"{code}: {message}")
        self.code = code
        self.message = message


@dataclass(frozen=True)
class RoleSession:
    credentials: Credentials
    expiration: datetime
    role_vrn: str
    session_vrn: str


_FRACTION = re.compile(r"\.(\d+)")


def parse_timestamp(text: str) -> datetime:
    """RFC 3339 timestamp with any fraction length; `fromisoformat` accepts only
    3 or 6 fractional digits before Python 3.11."""
    normalized = _FRACTION.sub(lambda m: "." + (m.group(1) + "000000")[:6], text.replace("Z", "+00:00"), count=1)
    return datetime.fromisoformat(normalized)


def assume_role(
    role_vrn: str,
    role_session_name: str,
    *,
    endpoint: str,
    credentials: Optional[CredentialsSource] = None,
    duration_seconds: Optional[int] = None,
    source_identity: Optional[str] = None,
    session_policy: Optional[Any] = None,
    timeout: float = 5.0,
) -> RoleSession:
    """`endpoint` is the full URL, for example
    `https://platform.vigrise.com/api/v1/sts/assume-role`. Callers request a
    new session before `expiration`."""
    request = {"role_vrn": role_vrn, "role_session_name": role_session_name}
    optional = {
        "duration_seconds": duration_seconds,
        "source_identity": source_identity,
        "session_policy": session_policy,
    }
    request.update({k: v for k, v in optional.items() if v is not None})
    body = json.dumps(request).encode()
    headers = {"content-type": "application/json"}
    headers.update(sign_request("POST", endpoint, headers, body, credentials_from(credentials), "sts"))
    http_request = urllib.request.Request(endpoint, data=body, headers=headers, method="POST")
    try:
        with urllib.request.urlopen(http_request, timeout=timeout) as response:
            reply = json.loads(response.read())
    except urllib.error.HTTPError as error:
        try:
            detail = json.loads(error.read()).get("error", {})
        except ValueError:
            detail = {}
        raise StsError(detail.get("code", str(error.code)), detail.get("message", "AssumeRole failed")) from None
    issued = reply["credentials"]
    return RoleSession(
        credentials=Credentials(issued["access_key_id"], issued["secret_access_key"], issued["session_token"]),
        expiration=parse_timestamp(issued["expiration"]),
        role_vrn=reply["assumed_role"]["role_vrn"],
        session_vrn=reply["assumed_role"]["session_vrn"],
    )

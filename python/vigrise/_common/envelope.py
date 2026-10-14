"""The error envelope of the service APIs: `{"error": {"code", "message", ...}}`."""

import json
from dataclasses import dataclass
from typing import Any, Dict, Optional


@dataclass(frozen=True)
class ErrorEnvelope:
    code: Optional[str]
    message: str
    # The resource an answer refers to.
    resource_id: Optional[str]
    # The fields besides `code`, `message` and `resource_id`.
    details: Dict[str, Any]


def parse_error_envelope(body: bytes) -> Optional[ErrorEnvelope]:
    """The envelope a response body carries; None when the body is not one."""
    try:
        envelope = json.loads(body).get("error")
    except (ValueError, AttributeError):
        return None
    if not isinstance(envelope, dict):
        return None
    details = dict(envelope)
    code = details.pop("code", None)
    message = details.pop("message", "")
    resource_id = details.pop("resource_id", None)
    return ErrorEnvelope(code if isinstance(code, str) else None, message, resource_id, details)

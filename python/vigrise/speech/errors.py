from typing import Any, Dict, Optional

from .._common.envelope import parse_error_envelope


class SpeechError(Exception):
    """Base of every error the Speech client raises."""


class SpeechServiceError(SpeechError):
    """The service answered with an error envelope, such as `VoiceNotFound`."""

    def __init__(self, status: int, code: str, message: str, resource_id: Optional[str] = None, details: Optional[Dict[str, Any]] = None):
        super().__init__(f"{code} ({status}): {message}")
        self.status = status
        self.code = code
        self.message = message
        # The resource an answer refers to, such as the generation an idempotency key already created.
        self.resource_id = resource_id
        # The other fields of the envelope, such as `quota` on limit refusals.
        self.details = details or {}


class SpeechTransportError(SpeechError):
    """The request could not be sent, or the answer could not be read."""


class SpeechSigningError(SpeechError):
    """The request could not be signed."""


class SpeechWaitTimeoutError(SpeechError):
    """A job did not finish within the time given to `wait_for_job`."""

    def __init__(self, last_job):
        super().__init__(f"job {last_job.id} did not finish in time")
        self.last_job = last_job


def error_from_response(status: int, body: bytes) -> SpeechServiceError:
    """The error an error response carries: `{"error": {"code", "message", ...}}`, or the status when the body is not that envelope."""
    envelope = parse_error_envelope(body)
    if envelope is not None:
        return SpeechServiceError(status, envelope.code or "UnknownError", envelope.message, envelope.resource_id, envelope.details)
    return SpeechServiceError(status, "UnknownError", body.decode("utf-8", "replace"))

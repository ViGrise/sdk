import json


class StorageError(Exception):
    """An error of a Storage call. `status` is the HTTP status of the answer; it is 0 and `code` is
    `TransportError` or `SigningError` when the request failed before an answer."""

    def __init__(self, status: int, code: str, message: str):
        super().__init__(f"{code} ({status}): {message}")
        self.status = status
        self.code = code
        self.message = message


def signing_error(message: str) -> StorageError:
    return StorageError(0, "SigningError", message)


def transport_error(message: str) -> StorageError:
    return StorageError(0, "TransportError", message)


def error_from_response(status: int, body: bytes) -> StorageError:
    """The error an error response carries: `{"error": {"code", "message"}}`, or the status when the body is not that envelope."""
    try:
        envelope = json.loads(body).get("error")
    except (ValueError, AttributeError):
        envelope = None
    if isinstance(envelope, dict) and isinstance(envelope.get("code"), str):
        return StorageError(status, envelope["code"], str(envelope.get("message", "")))
    return StorageError(status, "UnknownError", body.decode("utf-8", "replace"))

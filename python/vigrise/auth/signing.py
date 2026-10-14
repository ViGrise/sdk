import hashlib
import hmac

from .canonical import sha256_hex

ALGORITHM = "VGR4-HMAC-SHA256"
TERMINATOR = "vgr4_request"
TIMESTAMP_FORMAT = "%Y%m%dT%H%M%SZ"


def scope(date: str, service: str) -> str:
  return f"{date}/{service}/{TERMINATOR}"


def string_to_sign(timestamp: str, credential_scope: str, canonical_request: str) -> str:
  return f"{ALGORITHM}\n{timestamp}\n{credential_scope}\n{sha256_hex(canonical_request.encode())}"


def _hmac(key: bytes, message: str) -> bytes:
  return hmac.new(key, message.encode(), hashlib.sha256).digest()


def derive_signing_key(secret_access_key: str, date: str, service: str) -> bytes:
  return _hmac(_hmac(_hmac(f"VGR4{secret_access_key}".encode(), date), service), TERMINATOR)


def signature(signing_key: bytes, string_to_sign_value: str) -> str:
  return hmac.new(signing_key, string_to_sign_value.encode(), hashlib.sha256).hexdigest()

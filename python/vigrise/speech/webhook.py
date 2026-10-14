"""Verification of webhooks sent by the Speech API, which follow Standard Webhooks: HMAC-SHA256 over
`<id>.<timestamp>.<body>` with the base64 decoded secret, sent as space-separated `v1,<base64>` entries."""

import base64
import binascii
import hmac
import json
from datetime import datetime, timezone
from hashlib import sha256
from typing import Mapping, Optional, Union

from .errors import SpeechError
from .dto import Job, WebhookEvent, _time

SECRET_PREFIX = "whsec_"


class WebhookVerificationError(SpeechError):
  """A webhook was refused; `reason` is one of headers, timestamp, secret, signature or payload."""

  def __init__(self, reason: str, message: str):
    super().__init__(message)
    self.reason = reason


def _header(headers: Mapping[str, str], name: str) -> str:
  for key, value in headers.items():
    if key.lower() == name:
      if value:
        return value
  raise WebhookVerificationError("headers", f"the {name} header is missing")


def verify_webhook_signature(
  secret: str,
  headers: Mapping[str, str],
  raw_body: Union[str, bytes],
  tolerance: float = 300,
  now: Optional[datetime] = None,
) -> None:
  """Checks the signature headers of a webhook against the raw body; `tolerance` is in seconds."""
  webhook_id = _header(headers, "webhook-id")
  timestamp = _header(headers, "webhook-timestamp")
  signatures = _header(headers, "webhook-signature")
  try:
    seconds = int(timestamp)
  except ValueError:
    raise WebhookVerificationError("timestamp", "the webhook timestamp is not a number") from None
  current = (now or datetime.now(timezone.utc)).timestamp()
  if abs(int(current) - seconds) > tolerance:
    raise WebhookVerificationError("timestamp", "the webhook timestamp is outside the tolerance")
  encoded = secret[len(SECRET_PREFIX) :] if secret.startswith(SECRET_PREFIX) else secret
  try:
    key = base64.b64decode(encoded, validate=True)
  except (binascii.Error, ValueError):
    raise WebhookVerificationError("secret", "the webhook secret is not base64") from None
  body = raw_body.encode() if isinstance(raw_body, str) else raw_body
  expected = hmac.new(key, f"{webhook_id}.{timestamp}.".encode() + body, sha256).digest()
  matched = False
  for entry in signatures.split():
    if not entry.startswith("v1,"):
      continue
    try:
      candidate = base64.b64decode(entry[3:], validate=True)
    except (binascii.Error, ValueError):
      continue
    matched |= hmac.compare_digest(candidate, expected)
  if not matched:
    raise WebhookVerificationError("signature", "no webhook signature matches")


def verify_webhook(
  secret: str,
  headers: Mapping[str, str],
  raw_body: Union[str, bytes],
  tolerance: float = 300,
  now: Optional[datetime] = None,
) -> WebhookEvent:
  """Verifies a webhook and parses its event. `raw_body` is the request body as received, before any JSON parsing."""
  verify_webhook_signature(secret, headers, raw_body, tolerance, now)
  try:
    body = json.loads(raw_body)
    return WebhookEvent(
      id=_header(headers, "webhook-id"),
      type=body["type"],
      timestamp=_time(body["timestamp"]),
      data=Job.from_dict(body["data"]),
    )
  except (ValueError, KeyError, TypeError) as error:
    raise WebhookVerificationError(
      "payload", f"the webhook body is not an event: {error}"
    ) from None

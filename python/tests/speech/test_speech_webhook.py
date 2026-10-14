import base64
import hashlib
import hmac
import json
from datetime import datetime, timezone
from pathlib import Path

import pytest

from vigrise.speech import WebhookVerificationError, verify_webhook, verify_webhook_signature

CASES = json.loads((Path(__file__).parents[3] / "test-vectors" / "webhooks" / "standard-webhooks.json").read_text())["cases"]


def test_the_shared_vectors_exist():
    assert len(CASES) >= 10


@pytest.mark.parametrize("case", CASES, ids=lambda case: case["name"])
def test_shared_vectors(case):
    headers = {
        "webhook-id": case["id"],
        "webhook-timestamp": str(case["timestamp"]),
        "webhook-signature": case["signature"],
    }
    now = datetime.fromtimestamp(case["now"], tz=timezone.utc)

    def verify():
        verify_webhook_signature(case["secret"], headers, case["body"], tolerance=case["tolerance_seconds"], now=now)

    if case["valid"]:
        verify()
    else:
        with pytest.raises(WebhookVerificationError) as raised:
            verify()
        assert raised.value.reason == case["error"]


SECRET = "whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw"
BODY = json.dumps({
    "type": "job.succeeded",
    "timestamp": "2026-10-01T12:00:05Z",
    "data": {
        "id": "job_1", "object": "job", "status": "succeeded",
        "progress": {"pieces_done": 2, "pieces_total": 2, "characters_done": 10, "characters": 10},
        "characters_used": 10, "format": "wav", "webhook_url": None, "error": None, "generation_id": "gen_1",
        "created_by": "me", "created_at": "2026-10-01T12:00:00Z", "finished_at": "2026-10-01T12:00:05Z",
    },
})
NOW = datetime(2026, 10, 1, 12, 0, 6, tzinfo=timezone.utc)
TIMESTAMP = int(NOW.timestamp())


def signed_headers(body):
    key = base64.b64decode(SECRET[len("whsec_"):])
    mac = hmac.new(key, f"evt_1.{TIMESTAMP}.{body}".encode(), hashlib.sha256).digest()
    return {"Webhook-Id": "evt_1", "Webhook-Timestamp": str(TIMESTAMP), "Webhook-Signature": "v1," + base64.b64encode(mac).decode()}


def test_a_verified_webhook_becomes_an_event():
    event = verify_webhook(SECRET, signed_headers(BODY), BODY, now=NOW)
    assert (event.id, event.type) == ("evt_1", "job.succeeded")
    assert event.timestamp == datetime(2026, 10, 1, 12, 0, 5, tzinfo=timezone.utc)
    assert (event.data.id, event.data.generation_id) == ("job_1", "gen_1")


def test_bytes_are_accepted():
    assert verify_webhook(SECRET, signed_headers(BODY), BODY.encode(), now=NOW).id == "evt_1"


def test_a_tampered_body_is_refused():
    with pytest.raises(WebhookVerificationError):
        verify_webhook(SECRET, signed_headers(BODY), BODY.replace("job_1", "job_2"), now=NOW)


def test_webhook_verification_needs_no_httpx():
    import subprocess
    import sys

    code = (
        "import sys; sys.modules['httpx'] = None\n"
        "from vigrise.speech import verify_webhook, WebhookVerificationError\n"
        "import vigrise.speech.webhook\n"
    )
    result = subprocess.run([sys.executable, "-I", "-c", code], capture_output=True, text=True, cwd=str(Path(__file__).parents[2]),
                            env={"PYTHONPATH": str(Path(__file__).parents[2])})
    assert result.returncode == 0, result.stderr


def test_the_all_list_names_the_verifiers_and_the_clients():
    import vigrise.speech as speech

    assert {"verify_webhook", "verify_webhook_signature", "SpeechClient", "AsyncSpeechClient"} <= set(speech.__all__)
    assert all(hasattr(speech, name) for name in speech.__all__)

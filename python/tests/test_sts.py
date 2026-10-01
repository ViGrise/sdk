import json
import threading
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, HTTPServer

import httpx
import pytest
import requests

from vigrise_auth import Credentials
from vigrise_auth.httpx_auth import ViGriseHttpxAuth
from vigrise_auth.requests_auth import ViGriseAuth
from vigrise_auth.sts import StsError, assume_role

BACKEND = Credentials("VGAKBACKEND", "secret")
SESSION = Credentials("VGSTABCDEFGHIJKLMNOP", "session-secret", "v1.sealed")


def serve(status, reply):
    seen = []

    class Handler(BaseHTTPRequestHandler):
        def do_POST(self):
            body = self.rfile.read(int(self.headers["content-length"] or 0))
            seen.append((self.headers, body))
            payload = json.dumps(reply).encode()
            self.send_response(status)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def log_message(self, *args):
            pass

    httpd = HTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    return httpd, f"http://127.0.0.1:{httpd.server_port}/api/v1/sts/assume-role", seen


def test_assume_role_signs_under_the_sts_scope_and_returns_the_session():
    httpd, endpoint, seen = serve(200, {
        "credentials": {
            "access_key_id": SESSION.access_key_id,
            "secret_access_key": SESSION.secret_access_key,
            "session_token": SESSION.session_token,
            "expiration": "2026-09-26T10:30:00Z",
        },
        "assumed_role": {
            "role_vrn": "vrn:vigrise:iam:p:role/speech-client",
            "session_vrn": "vrn:vigrise:sts:p:assumed-role/speech-client/web",
        },
    })
    try:
        session = assume_role(
            "vrn:vigrise:iam:p:role/speech-client",
            "web",
            endpoint=endpoint,
            credentials=BACKEND,
            duration_seconds=900,
        )
    finally:
        httpd.shutdown()
    assert session.credentials == SESSION
    assert session.expiration == datetime(2026, 9, 26, 10, 30, tzinfo=timezone.utc)
    assert session.session_vrn == "vrn:vigrise:sts:p:assumed-role/speech-client/web"
    headers, body = seen[0]
    assert "Credential=VGAKBACKEND/" in headers["authorization"]
    assert "/sts/vgr4_request" in headers["authorization"]
    assert json.loads(body) == {
        "role_vrn": "vrn:vigrise:iam:p:role/speech-client",
        "role_session_name": "web",
        "duration_seconds": 900,
    }


def test_a_refused_assumption_raises_the_sts_error():
    httpd, endpoint, _ = serve(403, {"error": {"code": "AccessDenied", "message": "not trusted"}})
    try:
        with pytest.raises(StsError) as raised:
            assume_role("vrn:vigrise:iam:p:role/x", "web", endpoint=endpoint, credentials=BACKEND)
    finally:
        httpd.shutdown()
    assert raised.value.code == "AccessDenied"
    assert raised.value.message == "not trusted"


@pytest.mark.parametrize("client", ["requests", "httpx"])
def test_auth_classes_accept_a_credentials_provider(client):
    httpd, endpoint, seen = serve(200, {})
    calls = []

    def provider():
        calls.append(1)
        return SESSION

    try:
        if client == "requests":
            requests.post(endpoint, data=b"{}", auth=ViGriseAuth("speech", provider))
        else:
            httpx.post(endpoint, content=b"{}", auth=ViGriseHttpxAuth("speech", provider))
    finally:
        httpd.shutdown()
    assert calls == [1]
    assert seen[0][0]["x-vgr-security-token"] == "v1.sealed"
    assert "Credential=VGSTABCDEFGHIJKLMNOP/" in seen[0][0]["authorization"]


@pytest.mark.parametrize(
    "text",
    ["2026-09-26T10:30:00Z", "2026-09-26T10:30:00.123Z", "2026-09-26T10:30:00.123456789Z", "2026-09-26T10:30:00+00:00"],
)
def test_expirations_parse_whatever_their_fraction(text):
    from vigrise_auth.sts import parse_timestamp

    parsed = parse_timestamp(text)
    assert parsed.replace(microsecond=0) == datetime(2026, 9, 26, 10, 30, tzinfo=timezone.utc)

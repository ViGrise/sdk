import re
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer

import httpx
import pytest
import requests

from vigrise_auth import Credentials
from vigrise_auth.httpx_auth import ViGriseHttpxAuth
from vigrise_auth.requests_auth import ViGriseAuth

AUTH = re.compile(
    r"^VGR4-HMAC-SHA256 Credential=VGAKTEST/\d{8}/speech/vgr4_request, "
    r"SignedHeaders=content-type;host;x-vgr-content-sha256;x-vgr-date, Signature=[0-9a-f]{64}$"
)


@pytest.fixture
def server():
    seen = []

    class Handler(BaseHTTPRequestHandler):
        def do_POST(self):
            body = self.rfile.read(int(self.headers["content-length"]))
            seen.append((self.headers, body))
            self.send_response(200)
            self.end_headers()
            self.wfile.write(b"ok")

        def log_message(self, *args):
            pass

    httpd = HTTPServer(("127.0.0.1", 0), Handler)
    thread = threading.Thread(target=httpd.serve_forever, daemon=True)
    thread.start()
    yield f"http://127.0.0.1:{httpd.server_port}", seen
    httpd.shutdown()


CREDS = Credentials("VGAKTEST", "secret")


def test_requests_auth_signs(server):
    url, seen = server
    response = requests.post(f"{url}/v1/audio/speech", json={"input": "xin chào"}, auth=ViGriseAuth("speech", CREDS))
    assert response.text == "ok"
    headers, body = seen[0]
    assert AUTH.match(headers.get("Authorization")), headers.get("Authorization")
    assert body.decode() == '{"input": "xin ch\\u00e0o"}'


def test_httpx_auth_signs(server):
    url, seen = server
    response = httpx.post(f"{url}/v1/audio/speech", json={"input": "a"}, auth=ViGriseHttpxAuth("speech", CREDS))
    assert response.text == "ok"
    assert AUTH.match(seen[0][0].get("Authorization"))


def test_streaming_bodies_are_rejected():
    def chunks():
        yield b"x"

    with pytest.raises(ValueError, match="Streaming"):
        requests.Request("POST", "http://127.0.0.1:9/x", data=chunks(), auth=ViGriseAuth("speech", CREDS)).prepare()


def test_httpx_signs_repeated_signed_headers_as_sent(server):
    url, seen = server
    headers = httpx.Headers([("x-vgr-meta", "a"), ("x-vgr-meta", "b")])
    response = httpx.post(f"{url}/v1/audio/speech", content=b"x", headers=headers, auth=ViGriseHttpxAuth("speech", CREDS))
    assert response.text == "ok"
    sent_headers, body = seen[0]
    from vigrise_auth.canonical import canonical_request, sha256_hex
    from vigrise_auth.signing import derive_signing_key, scope, signature, string_to_sign

    auth = sent_headers.get("Authorization")
    signed = auth.split("SignedHeaders=")[1].split(",")[0].split(";")
    timestamp = sent_headers.get("x-vgr-date")
    pairs = [(k.lower(), v) for k, v in sent_headers.items()]
    creq = canonical_request("POST", "/v1/audio/speech", "", pairs, signed, sha256_hex(body))
    sts = string_to_sign(timestamp, scope(timestamp[:8], "speech"), creq)
    expected = signature(derive_signing_key("secret", timestamp[:8], "speech"), sts)
    assert auth.endswith(f"Signature={expected}")

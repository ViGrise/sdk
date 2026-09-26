import json
from datetime import datetime, timezone
from pathlib import Path

import pytest

from vigrise_auth import Credentials, sign_request
from vigrise_auth.canonical import canonical_request, sha256_hex
from vigrise_auth.signing import derive_signing_key, scope, signature, string_to_sign

VECTORS = sorted((Path(__file__).parent.parent.parent / "test-vectors" / "vgr4").glob("*.json"))


def test_vectors_exist():
    assert len(VECTORS) > 5


@pytest.mark.parametrize("path", VECTORS, ids=lambda p: p.name)
def test_primitives_match(path):
    v = json.loads(path.read_text())
    payload_hash = sha256_hex(v["request"]["body"].encode())
    assert payload_hash == v["expected"]["payload_hash"]
    headers = [tuple(h) for h in v["request"]["headers"]] + [("x-vgr-date", v["time"]), ("x-vgr-content-sha256", payload_hash)]
    if v["credentials"].get("session_token"):
        headers.append(("x-vgr-security-token", v["credentials"]["session_token"]))
    signed = v["expected"]["signed_headers"].split(";")
    creq = canonical_request(v["request"]["method"], v["request"]["path"], v["request"]["query"], headers, signed, payload_hash)
    assert creq == v["expected"]["canonical_request"]
    sts = string_to_sign(v["time"], scope(v["time"][:8], v["service"]), creq)
    assert sts == v["expected"]["string_to_sign"]
    key = derive_signing_key(v["credentials"]["secret_access_key"], v["time"][:8], v["service"])
    assert signature(key, sts) == v["expected"]["signature"]


@pytest.mark.parametrize("path", VECTORS, ids=lambda p: p.name)
def test_sign_request_matches(path):
    v = json.loads(path.read_text())
    host = next(value for name, value in v["request"]["headers"] if name.lower() == "host")
    query = f"?{v['request']['query']}" if v["request"]["query"] else ""
    signed = sign_request(
        v["request"]["method"],
        f"https://{host}{v['request']['path']}{query}",
        [tuple(h) for h in v["request"]["headers"]],
        v["request"]["body"],
        Credentials(
            v["credentials"]["access_key_id"],
            v["credentials"]["secret_access_key"],
            v["credentials"].get("session_token"),
        ),
        v["service"],
        datetime.strptime(v["time"], "%Y%m%dT%H%M%SZ").replace(tzinfo=timezone.utc),
    )
    assert signed["authorization"] == v["expected"]["authorization"]

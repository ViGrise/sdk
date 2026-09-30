#!/usr/bin/env python3
"""Reference AWS Signature Version 4 implementation for S3.

Reads inputs-sigv4/*.json and writes sigv4/*.json with the expected values. A
case may carry `published` values from the Amazon S3 documentation; the
generator fails when its own result differs from them.
"""
import hashlib, hmac, json, pathlib, sys

from checksums import checksum_b64
from generate import canonical_headers, canonical_query, canonical_uri, sha256_hex

ALGORITHM = "AWS4-HMAC-SHA256"
TERMINATOR = "aws4_request"


def hmac_bytes(key: bytes, msg: str) -> bytes:
    return hmac.new(key, msg.encode(), hashlib.sha256).digest()


def signing_key(secret: str, date: str, region: str, service: str) -> bytes:
    k = hmac_bytes(("AWS4" + secret).encode(), date)
    k = hmac_bytes(k, region)
    k = hmac_bytes(k, service)
    return hmac_bytes(k, TERMINATOR)


def sign(key: bytes, string_to_sign: str) -> str:
    return hmac.new(key, string_to_sign.encode(), hashlib.sha256).hexdigest()


def request_signature(case, key, scope):
    req = case["request"]
    headers = [list(h) for h in req["headers"]]
    signed = case["signed_headers"]
    payload_hash = req["payload_hash"]
    query = req["query"]
    if case.get("presigned"):
        query = "&".join(p for p in query.split("&") if not p.startswith("X-Amz-Signature="))
    creq = "\n".join([
        req["method"].upper(),
        canonical_uri(req["path"]),
        canonical_query(query),
        canonical_headers(headers, signed),
        ";".join(signed),
        payload_hash,
    ])
    sts = "\n".join([ALGORITHM, case["time"], scope, sha256_hex(creq.encode())])
    return creq, sts, sign(key, sts)


def chunk_data(chunk):
    return chunk["byte"].encode() * chunk["length"]


def chunk_signatures(case, key, scope, seed):
    previous = seed
    out = []
    empty = sha256_hex(b"")
    for chunk in case["chunks"] + [{"byte": "a", "length": 0}]:
        sts = "\n".join([
            "AWS4-HMAC-SHA256-PAYLOAD", case["time"], scope, previous, empty,
            sha256_hex(chunk_data(chunk)),
        ])
        previous = sign(key, sts)
        out.append(previous)
    return out, previous


def trailer_signature(case, key, scope, previous):
    lines = "".join(f"{name}:{value}\n" for name, value in case["trailer"])
    sts = "\n".join(["AWS4-HMAC-SHA256-TRAILER", case["time"], scope, previous, sha256_hex(lines.encode())])
    return sign(key, sts)


def resolve_trailer(case):
    """Replaces a `crc32c` placeholder with the checksum of the chunk data."""
    body = b"".join(chunk_data(chunk) for chunk in case["chunks"])
    return [[name, checksum_b64(value[1:].upper(), body) if value.startswith("@") else value]
            for name, value in case["trailer"]]


def build(case):
    if "trailer" in case:
        case = {**case, "trailer": resolve_trailer(case)}
    creds = case["credentials"]
    date = case["time"][:8]
    scope = f"{date}/{case['region']}/{case['service']}/{TERMINATOR}"
    key = signing_key(creds["secret_access_key"], date, case["region"], case["service"])
    creq, sts, signature = request_signature(case, key, scope)
    expected = {
        "canonical_request": creq,
        "string_to_sign": sts,
        "signing_key": key.hex(),
        "signature": signature,
    }
    if not case.get("presigned"):
        expected["authorization"] = (
            f"{ALGORITHM} Credential={creds['access_key_id']}/{scope}, "
            f"SignedHeaders={';'.join(case['signed_headers'])}, Signature={signature}"
        )
    if "chunks" in case:
        chunk_sigs, last = chunk_signatures(case, key, scope, signature)
        expected["chunk_signatures"] = chunk_sigs
        if "trailer" in case:
            expected["trailer_signature"] = trailer_signature(case, key, scope, last)
    for name, value in case.get("published", {}).items():
        got = expected[name]
        if got != value:
            sys.exit(f"{case['description']}: {name} differs from the published value\n  got {got}\n  published {value}")
    return {**case, "expected": expected}


def main():
    root = pathlib.Path(__file__).parent
    (root / "sigv4").mkdir(exist_ok=True)
    for path in sorted((root / "inputs-sigv4").glob("*.json")):
        case = json.loads(path.read_text())
        (root / "sigv4" / path.name).write_text(json.dumps(build(case), indent=2, ensure_ascii=False) + "\n")
        print(f"wrote sigv4/{path.name}")


if __name__ == "__main__":
    sys.exit(main())

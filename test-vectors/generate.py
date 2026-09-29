#!/usr/bin/env python3
"""Reference VGR4 implementation. Reads inputs/*.json and writes vgr4/*.json with expected values."""
import hashlib, hmac, json, pathlib, sys, urllib.parse

ALGORITHM = "VGR4-HMAC-SHA256"
TERMINATOR = "vgr4_request"
UNRESERVED = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.~"


def reencode(raw: str) -> str:
    """Percent-decodes to bytes, then encodes every byte outside the unreserved set."""
    data = urllib.parse.unquote_to_bytes(raw)
    return "".join(chr(b) if chr(b) in UNRESERVED else f"%{b:02X}" for b in data)


def canonical_uri(raw_path: str) -> str:
    if raw_path == "":
        return "/"
    return "/".join(reencode(seg) for seg in raw_path.split("/"))


def canonical_query(raw_query: str) -> str:
    if not raw_query:
        return ""
    pairs = []
    for part in raw_query.split("&"):
        if part == "":
            continue
        k, _, v = part.partition("=")
        pairs.append((reencode(k), reencode(v)))
    pairs.sort()
    return "&".join(f"{k}={v}" for k, v in pairs)


def canonical_headers(headers, signed):
    out = ""
    for name in signed:
        values = [" ".join(v.split()) for (n, v) in headers if n.lower() == name]
        out += f"{name}:{','.join(values)}\n"
    return out


def sha256_hex(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def hmac_sha256(key: bytes, msg: str) -> bytes:
    return hmac.new(key, msg.encode(), hashlib.sha256).digest()


def build(case):
    req = case["request"]
    body = req["body"].encode()
    # A streamed body is signed as UNSIGNED-PAYLOAD instead of its hash.
    payload_hash = req.get("payload_hash") or sha256_hex(body)
    time = case["time"]
    date = time[:8]
    service = case["service"]
    headers = list(req["headers"]) + [["x-vgr-date", time], ["x-vgr-content-sha256", payload_hash]]
    session_token = case["credentials"].get("session_token")
    if session_token:
        headers.append(["x-vgr-security-token", session_token])
    # Signers sign host, content-type and every x-vgr-* header; others are sent unsigned.
    signed = sorted({n.lower() for n, _ in headers if n.lower() in ("host", "content-type") or n.lower().startswith("x-vgr-")})
    creq = "\n".join([
        req["method"].upper(),
        canonical_uri(req["path"]),
        canonical_query(req["query"]),
        canonical_headers(headers, signed),
        ";".join(signed),
        payload_hash,
    ])
    scope = f"{date}/{service}/{TERMINATOR}"
    sts = "\n".join([ALGORITHM, time, scope, sha256_hex(creq.encode())])
    k = hmac_sha256(("VGR4" + case["credentials"]["secret_access_key"]).encode(), date)
    k = hmac_sha256(k, service)
    k = hmac_sha256(k, TERMINATOR)
    sig = hmac.new(k, sts.encode(), hashlib.sha256).hexdigest()
    auth = (f"{ALGORITHM} Credential={case['credentials']['access_key_id']}/{scope}, "
            f"SignedHeaders={';'.join(signed)}, Signature={sig}")
    out = dict(case)
    out["expected"] = {
        "payload_hash": payload_hash,
        "signed_headers": ";".join(signed),
        "canonical_request": creq,
        "string_to_sign": sts,
        "signature": sig,
        "authorization": auth,
    }
    return out


def main():
    root = pathlib.Path(__file__).parent
    (root / "vgr4").mkdir(exist_ok=True)
    for path in sorted((root / "inputs").glob("*.json")):
        case = json.loads(path.read_text())
        (root / "vgr4" / path.name).write_text(json.dumps(build(case), indent=2, ensure_ascii=False) + "\n")
        print(f"wrote vgr4/{path.name}")


if __name__ == "__main__":
    sys.exit(main())

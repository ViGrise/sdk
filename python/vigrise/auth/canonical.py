"""Canonical request construction. Must stay byte-for-byte identical to
test-vectors/generate.py and the Rust and TypeScript SDKs."""

import hashlib
import urllib.parse
from typing import List, Sequence, Tuple

UNRESERVED = frozenset(b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.~")

HeaderList = Sequence[Tuple[str, str]]


def sha256_hex(data: bytes) -> str:
  return hashlib.sha256(data).hexdigest()


def _reencode(raw: str) -> str:
  """Decodes to bytes (malformed escapes stay literal) and encodes every byte
  outside the RFC 3986 unreserved set; `+` is a literal."""
  return "".join(
    chr(b) if b in UNRESERVED else f"%{b:02X}" for b in urllib.parse.unquote_to_bytes(raw)
  )


def canonical_uri(raw_path: str) -> str:
  return "/" if raw_path == "" else "/".join(_reencode(segment) for segment in raw_path.split("/"))


def canonical_query(raw_query: str) -> str:
  pairs: List[Tuple[str, str]] = []
  for part in raw_query.split("&"):
    if part == "":
      continue
    key, _, value = part.partition("=")
    pairs.append((_reencode(key), _reencode(value)))
  pairs.sort()
  return "&".join(f"{k}={v}" for k, v in pairs)


def canonical_headers(headers: HeaderList, signed_headers: Sequence[str]) -> str:
  """One `name:value\\n` line per signed header; repeated values join with `,`."""
  lines = []
  for name in signed_headers:
    values = [" ".join(v.split()) for n, v in headers if n.lower() == name]
    if not values:
      raise ValueError(f"Signed header {name} is missing from the request")
    lines.append(f"{name}:{','.join(values)}\n")
  return "".join(lines)


def canonical_request(
  method: str,
  raw_path: str,
  raw_query: str,
  headers: HeaderList,
  signed_headers: Sequence[str],
  payload_hash: str,
) -> str:
  return "\n".join(
    [
      method.upper(),
      canonical_uri(raw_path),
      canonical_query(raw_query),
      canonical_headers(headers, signed_headers),
      ";".join(signed_headers),
      payload_hash,
    ]
  )

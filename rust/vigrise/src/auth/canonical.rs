//! Canonical request construction. Every function here must stay byte-for-byte
//! identical to `test-vectors/generate.py`.

use http::HeaderMap;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, percent_encode};
use sha2::{Digest, Sha256};

use crate::auth::error::AuthError;

/// RFC 3986 unreserved characters are left as-is; every other byte is encoded.
const UNRESERVED: &AsciiSet = &NON_ALPHANUMERIC
  .remove(b'-')
  .remove(b'_')
  .remove(b'.')
  .remove(b'~');

/// Decodes to raw bytes and re-encodes, so any client-side encoding of the same
/// bytes canonicalizes identically. `+` is a literal plus.
fn reencode(raw: &str) -> String {
  let bytes: Vec<u8> = percent_decode_str(raw).collect();
  percent_encode(&bytes, UNRESERVED).to_string()
}

/// Hex-encoded SHA-256 of `data`.
pub fn hex_sha256(data: &[u8]) -> String {
  hex::encode(Sha256::digest(data))
}

/// The canonical URI of a raw request path: every segment is decoded and
/// re-encoded with the unreserved character set.
pub fn canonical_uri(raw_path: &str) -> String {
  if raw_path.is_empty() {
    return "/".to_string();
  }
  raw_path
    .split('/')
    .map(reencode)
    .collect::<Vec<_>>()
    .join("/")
}

/// The canonical query string: pairs are re-encoded and sorted by key, then
/// value.
pub fn canonical_query(raw_query: &str) -> String {
  let mut pairs: Vec<(String, String)> = raw_query
    .split('&')
    .filter(|part| !part.is_empty())
    .map(|part| {
      let (key, value) = part.split_once('=').unwrap_or((part, ""));
      (reencode(key), reencode(value))
    })
    .collect();
  pairs.sort();
  pairs
    .iter()
    .map(|(key, value)| format!("{key}={value}"))
    .collect::<Vec<_>>()
    .join("&")
}

/// One `name:value\n` line per signed header. Multiple values of a header are
/// joined with `,`; whitespace runs inside a value collapse to one space.
pub fn canonical_headers(
  headers: &HeaderMap,
  signed_headers: &[String],
) -> Result<String, AuthError> {
  let mut out = String::new();
  for name in signed_headers {
    let mut values = Vec::new();
    for value in headers.get_all(name.as_str()) {
      let value = value.to_str().map_err(|_| {
        AuthError::IncompleteSignature(format!("Signed header {name} is not valid ASCII"))
      })?;
      values.push(value.split_whitespace().collect::<Vec<_>>().join(" "));
    }
    if values.is_empty() {
      return Err(AuthError::IncompleteSignature(format!(
        "Signed header {name} is missing from the request"
      )));
    }
    out.push_str(name);
    out.push(':');
    out.push_str(&values.join(","));
    out.push('\n');
  }
  Ok(out)
}

/// The canonical request that the string to sign hashes.
pub fn canonical_request(
  method: &str,
  raw_path: &str,
  raw_query: &str,
  headers: &HeaderMap,
  signed_headers: &[String],
  payload_hash: &str,
) -> Result<String, AuthError> {
  Ok(
    [
      method.to_ascii_uppercase(),
      canonical_uri(raw_path),
      canonical_query(raw_query),
      canonical_headers(headers, signed_headers)?,
      signed_headers.join(";"),
      payload_hash.to_string(),
    ]
    .join("\n"),
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn empty_path_is_slash() {
    assert_eq!(canonical_uri(""), "/");
    assert_eq!(canonical_uri("/"), "/");
  }

  #[test]
  fn path_segments_are_reencoded_once() {
    assert_eq!(canonical_uri("/a b/%7Ex"), "/a%20b/~x");
    assert_eq!(canonical_uri("/a%2Fb"), "/a%2Fb");
  }

  #[test]
  fn query_is_sorted_and_plus_is_literal() {
    assert_eq!(canonical_query("b=2&a=&a=1&q=a+b"), "a=&a=1&b=2&q=a%2Bb");
    assert_eq!(canonical_query(""), "");
  }

  #[test]
  fn escapes_normalize_to_uppercase_bytes() {
    assert_eq!(canonical_uri("/a%2fb/%ff"), "/a%2Fb/%FF");
    assert_eq!(canonical_query("k=%ff&j=%e1%bb%8d"), "j=%E1%BB%8D&k=%FF");
  }

  #[test]
  fn repeated_header_values_join_with_comma() {
    let mut headers = HeaderMap::new();
    headers.append("x-vgr-meta", " a  b ".parse().unwrap());
    headers.append("x-vgr-meta", "c".parse().unwrap());
    assert_eq!(
      canonical_headers(&headers, &["x-vgr-meta".to_string()]).unwrap(),
      "x-vgr-meta:a b,c\n"
    );
  }

  #[test]
  fn missing_signed_header_is_rejected() {
    let err = canonical_headers(&HeaderMap::new(), &["host".to_string()]).unwrap_err();
    assert_eq!(err.code(), "IncompleteSignature");
  }
}

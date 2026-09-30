//! AWS Signature Version 4 as S3 uses it: header authorization, presigned
//! query authorization and the chunk signatures of aws-chunked uploads.
//!
//! The scope is `<yyyymmdd>/<region>/<service>/aws4_request`. Canonical
//! requests share the construction of [`crate::canonical`]: the URI path is
//! encoded once and the query is sorted and encoded per SigV4.

use std::fmt;

use http::HeaderMap;
use percent_encoding::percent_decode_str;
use sha2::{Digest, Sha256};

use crate::canonical::hex_sha256;
use crate::error::AuthError;
use crate::signing::{hmac_sha256, signature};

pub const ALGORITHM: &str = "AWS4-HMAC-SHA256";
pub const TERMINATOR: &str = "aws4_request";
pub const UNSIGNED_PAYLOAD: &str = "UNSIGNED-PAYLOAD";
pub const STREAMING_UNSIGNED_PAYLOAD_TRAILER: &str = "STREAMING-UNSIGNED-PAYLOAD-TRAILER";
pub const STREAMING_SIGNED_PAYLOAD: &str = "STREAMING-AWS4-HMAC-SHA256-PAYLOAD";
pub const STREAMING_SIGNED_PAYLOAD_TRAILER: &str = "STREAMING-AWS4-HMAC-SHA256-PAYLOAD-TRAILER";
/// Longest validity of a presigned request, in seconds.
pub const MAX_PRESIGN_EXPIRES: i64 = 604_800;

/// The credential scope of a signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
  pub date: String,
  pub region: String,
  pub service: String,
}

impl Scope {
  pub fn new(date: &str, region: &str, service: &str) -> Self {
    Self {
      date: date.to_string(),
      region: region.to_string(),
      service: service.to_string(),
    }
  }

  /// Parses `<date>/<region>/<service>/aws4_request`.
  pub fn parse(value: &str) -> Option<Self> {
    let parts: Vec<&str> = value.split('/').collect();
    let [date, region, service, terminator] = parts.as_slice() else {
      return None;
    };
    let well_formed = *terminator == TERMINATOR
      && date.len() == 8
      && date.bytes().all(|b| b.is_ascii_digit())
      && !region.is_empty()
      && !service.is_empty();
    well_formed.then(|| Self::new(date, region, service))
  }
}

impl fmt::Display for Scope {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
      f,
      "{}/{}/{}/{TERMINATOR}",
      self.date, self.region, self.service
    )
  }
}

/// `AWS4` + secret -> date -> region -> service -> `aws4_request`.
pub fn derive_signing_key(
  secret_access_key: &str,
  date: &str,
  region: &str,
  service: &str,
) -> [u8; 32] {
  let k_date = hmac_sha256(format!("AWS4{secret_access_key}").as_bytes(), date);
  let k_region = hmac_sha256(&k_date, region);
  let k_service = hmac_sha256(&k_region, service);
  hmac_sha256(&k_service, TERMINATOR)
}

/// Decodes a signing key handed out as hex.
pub fn decode_signing_key(hex_key: &str) -> Option<[u8; 32]> {
  hex::decode(hex_key).ok()?.try_into().ok()
}

pub fn string_to_sign(timestamp: &str, scope: &Scope, canonical_request: &str) -> String {
  format!(
    "{ALGORITHM}\n{timestamp}\n{scope}\n{}",
    hex_sha256(canonical_request.as_bytes())
  )
}

pub fn canonical_request(
  method: &str,
  raw_path: &str,
  raw_query: &str,
  headers: &HeaderMap,
  signed_headers: &[String],
  payload_hash: &str,
) -> Result<String, AuthError> {
  crate::canonical::canonical_request(
    method,
    raw_path,
    raw_query,
    headers,
    signed_headers,
    payload_hash,
  )
}

/// The query of a presigned request as it is signed: without the signature.
pub fn query_without_signature(raw_query: &str) -> String {
  raw_query
    .split('&')
    .filter(|part| !part.is_empty())
    .filter(|part| decode(part.split_once('=').map_or(*part, |(key, _)| key)) != "X-Amz-Signature")
    .collect::<Vec<_>>()
    .join("&")
}

/// String to sign of one chunk of an aws-chunked body; `chunk_hash` is the hex
/// SHA-256 of the chunk data and `previous` the signature that came before it.
pub fn chunk_string_to_sign(
  timestamp: &str,
  scope: &Scope,
  previous: &str,
  chunk_hash: &str,
) -> String {
  format!(
    "AWS4-HMAC-SHA256-PAYLOAD\n{timestamp}\n{scope}\n{previous}\n{}\n{chunk_hash}",
    hex_sha256(b"")
  )
}

pub fn chunk_signature(
  signing_key: &[u8; 32],
  timestamp: &str,
  scope: &Scope,
  previous: &str,
  chunk_hash: &str,
) -> String {
  signature(
    signing_key,
    &chunk_string_to_sign(timestamp, scope, previous, chunk_hash),
  )
}

/// String to sign of the trailing headers of an aws-chunked body, given as
/// the header lines each ending in `\n`.
pub fn trailer_string_to_sign(
  timestamp: &str,
  scope: &Scope,
  previous: &str,
  trailer_lines: &[u8],
) -> String {
  format!(
    "AWS4-HMAC-SHA256-TRAILER\n{timestamp}\n{scope}\n{previous}\n{}",
    hex::encode(Sha256::digest(trailer_lines))
  )
}

pub fn trailer_signature(
  signing_key: &[u8; 32],
  timestamp: &str,
  scope: &Scope,
  previous: &str,
  trailer_lines: &[u8],
) -> String {
  signature(
    signing_key,
    &trailer_string_to_sign(timestamp, scope, previous, trailer_lines),
  )
}

fn decode(raw: &str) -> String {
  percent_decode_str(raw).decode_utf8_lossy().into_owned()
}

fn sorted_lowercase_headers(value: &str) -> Result<Vec<String>, AuthError> {
  let headers: Vec<String> = value.split(';').map(str::to_string).collect();
  let well_formed = headers.windows(2).all(|pair| pair[0] < pair[1])
    && headers
      .iter()
      .all(|h| !h.is_empty() && h.bytes().all(|b| !b.is_ascii_uppercase()));
  if !well_formed || !headers.iter().any(|h| h == "host") {
    return Err(AuthError::IncompleteSignature(
      "SignedHeaders must be lowercase, sorted and include host".into(),
    ));
  }
  Ok(headers)
}

fn signature_hex(value: &str) -> Result<String, AuthError> {
  if value.len() == 64
    && value
      .bytes()
      .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
  {
    Ok(value.to_string())
  } else {
    Err(AuthError::IncompleteSignature(
      "Signature must be 64 lowercase hex characters".into(),
    ))
  }
}

fn parse_credential(value: &str) -> Result<(String, Scope), AuthError> {
  let malformed = || {
    AuthError::IncompleteSignature(
      "Credential must be <access_key_id>/<date>/<region>/<service>/aws4_request".into(),
    )
  };
  let (access_key_id, scope) = value.split_once('/').ok_or_else(malformed)?;
  if access_key_id.is_empty() {
    return Err(malformed());
  }
  Ok((
    access_key_id.to_string(),
    Scope::parse(scope).ok_or_else(malformed)?,
  ))
}

/// The `Authorization` header of a SigV4 request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderAuthorization {
  pub access_key_id: String,
  pub scope: Scope,
  pub signed_headers: Vec<String>,
  pub signature: String,
}

impl HeaderAuthorization {
  pub fn parse(value: &str) -> Result<Self, AuthError> {
    let incomplete = |message: &str| AuthError::IncompleteSignature(message.to_string());
    let rest = value
      .strip_prefix(ALGORITHM)
      .and_then(|rest| rest.strip_prefix(' '))
      .ok_or_else(|| incomplete("Authorization header must start with AWS4-HMAC-SHA256"))?;
    let (mut credential, mut signed_headers, mut signature) = (None, None, None);
    for part in rest.split(',') {
      let (key, val) = part
        .trim()
        .split_once('=')
        .ok_or_else(|| incomplete("Authorization header components must be key=value"))?;
      match key {
        "Credential" => credential = Some(val),
        "SignedHeaders" => signed_headers = Some(val),
        "Signature" => signature = Some(val),
        _ => return Err(incomplete("Authorization header has an unknown component")),
      }
    }
    let (access_key_id, scope) = parse_credential(
      credential.ok_or_else(|| incomplete("Authorization header is missing Credential"))?,
    )?;
    Ok(Self {
      access_key_id,
      scope,
      signed_headers: sorted_lowercase_headers(
        signed_headers
          .ok_or_else(|| incomplete("Authorization header is missing SignedHeaders"))?,
      )?,
      signature: signature_hex(
        signature.ok_or_else(|| incomplete("Authorization header is missing Signature"))?,
      )?,
    })
  }
}

impl fmt::Display for HeaderAuthorization {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
      f,
      "{ALGORITHM} Credential={}/{}, SignedHeaders={}, Signature={}",
      self.access_key_id,
      self.scope,
      self.signed_headers.join(";"),
      self.signature
    )
  }
}

/// The authentication parameters of a presigned request's query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Presigned {
  pub access_key_id: String,
  pub scope: Scope,
  pub timestamp: String,
  /// Validity in seconds from `timestamp`, between 1 and [`MAX_PRESIGN_EXPIRES`].
  pub expires: i64,
  pub signed_headers: Vec<String>,
  pub signature: String,
  pub security_token: Option<String>,
}

impl Presigned {
  /// `Ok(None)` when the query carries no `X-Amz-Algorithm`, so the request
  /// is not presigned.
  pub fn parse(raw_query: &str) -> Result<Option<Self>, AuthError> {
    let (mut algorithm, mut credential, mut timestamp, mut expires) = (None, None, None, None);
    let (mut signed_headers, mut signature, mut security_token) = (None, None, None);
    for part in raw_query.split('&').filter(|p| !p.is_empty()) {
      let (key, value) = part.split_once('=').unwrap_or((part, ""));
      let value = decode(value);
      match decode(key).as_str() {
        "X-Amz-Algorithm" => algorithm = Some(value),
        "X-Amz-Credential" => credential = Some(value),
        "X-Amz-Date" => timestamp = Some(value),
        "X-Amz-Expires" => expires = Some(value),
        "X-Amz-SignedHeaders" => signed_headers = Some(value),
        "X-Amz-Signature" => signature = Some(value),
        "X-Amz-Security-Token" => security_token = Some(value),
        _ => {}
      }
    }
    let Some(algorithm) = algorithm else {
      return Ok(None);
    };
    let missing = |name: &str| {
      AuthError::IncompleteSignature(format!("Query-string authentication requires {name}"))
    };
    if algorithm != ALGORITHM {
      return Err(AuthError::IncompleteSignature(
        "X-Amz-Algorithm must be AWS4-HMAC-SHA256".into(),
      ));
    }
    let (access_key_id, scope) =
      parse_credential(&credential.ok_or_else(|| missing("X-Amz-Credential"))?)?;
    let timestamp = timestamp.ok_or_else(|| missing("X-Amz-Date"))?;
    if !timestamp_shape_ok(&timestamp) || !timestamp.starts_with(&scope.date) {
      return Err(AuthError::IncompleteSignature(
        "X-Amz-Date must be a basic ISO 8601 timestamp on the credential scope date".into(),
      ));
    }
    let expires: i64 = expires
      .ok_or_else(|| missing("X-Amz-Expires"))?
      .parse()
      .map_err(|_| AuthError::IncompleteSignature("X-Amz-Expires must be a number".into()))?;
    if !(1..=MAX_PRESIGN_EXPIRES).contains(&expires) {
      return Err(AuthError::IncompleteSignature(format!(
        "X-Amz-Expires must be between 1 and {MAX_PRESIGN_EXPIRES} seconds"
      )));
    }
    Ok(Some(Self {
      access_key_id,
      scope,
      timestamp,
      expires,
      signed_headers: sorted_lowercase_headers(
        &signed_headers.ok_or_else(|| missing("X-Amz-SignedHeaders"))?,
      )?,
      signature: signature_hex(&signature.ok_or_else(|| missing("X-Amz-Signature"))?)?,
      security_token,
    }))
  }
}

/// `yyyymmddThhmmssZ`.
pub fn timestamp_shape_ok(timestamp: &str) -> bool {
  timestamp.len() == 16
    && timestamp.bytes().enumerate().all(|(i, b)| match i {
      8 => b == b'T',
      15 => b == b'Z',
      _ => b.is_ascii_digit(),
    })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn scope_round_trips() {
    let scope = Scope::parse("20130524/us-east-1/s3/aws4_request").unwrap();
    assert_eq!(scope, Scope::new("20130524", "us-east-1", "s3"));
    assert_eq!(scope.to_string(), "20130524/us-east-1/s3/aws4_request");
    assert!(Scope::parse("2013/us-east-1/s3/aws4_request").is_none());
    assert!(Scope::parse("20130524/us-east-1/s3/vgr4_request").is_none());
    assert!(Scope::parse("20130524//s3/aws4_request").is_none());
  }

  #[test]
  fn header_authorization_rejects_other_algorithms_and_bad_signed_headers() {
    let sig = "a".repeat(64);
    let header = |signed: &str| {
      format!(
        "AWS4-HMAC-SHA256 Credential=AK/20130524/us-east-1/s3/aws4_request,SignedHeaders={signed},Signature={sig}"
      )
    };
    assert!(HeaderAuthorization::parse(&header("host;x-amz-date")).is_ok());
    assert!(HeaderAuthorization::parse(&header("x-amz-date;host")).is_err());
    assert!(HeaderAuthorization::parse(&header("x-amz-date")).is_err());
    assert!(HeaderAuthorization::parse("VGR4-HMAC-SHA256 Credential=a/b/c/d").is_err());
  }

  #[test]
  fn presigned_expiry_is_bounded() {
    let query = |expires: &str| {
      format!(
        "X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=AK%2F20130524%2Fus-east-1%2Fs3%2Faws4_request&X-Amz-Date=20130524T000000Z&X-Amz-Expires={expires}&X-Amz-SignedHeaders=host&X-Amz-Signature={}",
        "b".repeat(64)
      )
    };
    assert_eq!(
      Presigned::parse(&query("604800")).unwrap().unwrap().expires,
      604_800
    );
    assert!(Presigned::parse(&query("604801")).is_err());
    assert!(Presigned::parse(&query("0")).is_err());
    assert!(Presigned::parse(&query("soon")).is_err());
    assert!(Presigned::parse("prefix=a").unwrap().is_none());
  }

  #[test]
  fn presigned_carries_the_session_token() {
    let query = format!(
      "X-Amz-Algorithm=AWS4-HMAC-SHA256&X-Amz-Credential=AK%2F20130524%2Fus-east-1%2Fs3%2Faws4_request&X-Amz-Date=20130524T000000Z&X-Amz-Expires=60&X-Amz-Security-Token=abc%2Bdef&X-Amz-SignedHeaders=host&X-Amz-Signature={}",
      "b".repeat(64)
    );
    let parsed = Presigned::parse(&query).unwrap().unwrap();
    assert_eq!(parsed.security_token.as_deref(), Some("abc+def"));
  }

  #[test]
  fn query_without_signature_keeps_the_other_parameters_verbatim() {
    assert_eq!(
      query_without_signature("a=1&X-Amz-Signature=ff&b=2"),
      "a=1&b=2"
    );
    assert_eq!(query_without_signature("X-Amz-Signature=ff"), "");
  }

  #[test]
  fn signing_key_decodes_from_hex() {
    let key = derive_signing_key("secret", "20130524", "us-east-1", "s3");
    assert_eq!(decode_signing_key(&hex::encode(key)), Some(key));
    assert_eq!(decode_signing_key("abcd"), None);
  }
}

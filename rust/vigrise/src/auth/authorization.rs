use std::fmt;

use crate::auth::error::AuthError;
use crate::auth::signing::{ALGORITHM, TERMINATOR, scope};

/// Headers every VGR4 signature must cover.
pub const REQUIRED_SIGNED_HEADERS: [&str; 3] = ["host", "x-vgr-content-sha256", "x-vgr-date"];

/// A parsed VGR4 `Authorization` header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationHeader {
  /// Access key ID of the signer.
  pub access_key_id: String,
  /// Credential scope date, `yyyymmdd`.
  pub date: String,
  /// Credential scope service.
  pub service: String,
  /// Lowercase names of the signed headers, in header order.
  pub signed_headers: Vec<String>,
  /// Hex-encoded signature.
  pub signature: String,
}

impl AuthorizationHeader {
  /// The credential scope, `<date>/<service>/vgr4_request`.
  pub fn scope(&self) -> String {
    scope(&self.date, &self.service)
  }

  /// Parses the value of an `Authorization` header.
  pub fn parse(value: &str) -> Result<Self, AuthError> {
    let incomplete = |message: &str| AuthError::IncompleteSignature(message.to_string());
    let rest = value
      .strip_prefix(ALGORITHM)
      .and_then(|rest| rest.strip_prefix(' '))
      .ok_or_else(|| incomplete("Authorization header must start with VGR4-HMAC-SHA256"))?;

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
    let credential =
      credential.ok_or_else(|| incomplete("Authorization header is missing Credential"))?;
    let signed_headers =
      signed_headers.ok_or_else(|| incomplete("Authorization header is missing SignedHeaders"))?;
    let signature =
      signature.ok_or_else(|| incomplete("Authorization header is missing Signature"))?;

    let credential_parts: Vec<&str> = credential.split('/').collect();
    let [access_key_id, date, service, terminator] = credential_parts.as_slice() else {
      return Err(incomplete(
        "Credential must be <access_key_id>/<date>/<service>/vgr4_request",
      ));
    };
    if *terminator != TERMINATOR
      || access_key_id.is_empty()
      || service.is_empty()
      || date.len() != 8
      || !date.bytes().all(|b| b.is_ascii_digit())
    {
      return Err(incomplete(
        "Credential must be <access_key_id>/<date>/<service>/vgr4_request",
      ));
    }

    let signed_headers: Vec<String> = signed_headers.split(';').map(str::to_string).collect();
    let well_formed = signed_headers.windows(2).all(|pair| pair[0] < pair[1])
      && signed_headers
        .iter()
        .all(|h| !h.is_empty() && h.bytes().all(|b| !b.is_ascii_uppercase()));
    if !well_formed {
      return Err(incomplete(
        "SignedHeaders must be lowercase and strictly sorted",
      ));
    }
    for required in REQUIRED_SIGNED_HEADERS {
      if !signed_headers.iter().any(|h| h == required) {
        return Err(AuthError::IncompleteSignature(format!(
          "SignedHeaders must include {required}"
        )));
      }
    }

    if signature.len() != 64
      || !signature
        .bytes()
        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
      return Err(incomplete("Signature must be 64 lowercase hex characters"));
    }

    Ok(Self {
      access_key_id: access_key_id.to_string(),
      date: date.to_string(),
      service: service.to_string(),
      signed_headers,
      signature: signature.to_string(),
    })
  }
}

impl fmt::Display for AuthorizationHeader {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
      f,
      "{ALGORITHM} Credential={}/{}, SignedHeaders={}, Signature={}",
      self.access_key_id,
      self.scope(),
      self.signed_headers.join(";"),
      self.signature
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const SIG: &str = "352eb0275aa181809f19bcad5f8bc34746bb2ecd27fa1b865222115654015e69";

  fn header(signed: &str) -> String {
    format!(
      "VGR4-HMAC-SHA256 Credential=VGAKEXAMPLE7TESTKEY/20261001/speech/vgr4_request, SignedHeaders={signed}, Signature={SIG}"
    )
  }

  #[test]
  fn parses_and_displays_identically() {
    let raw = header("host;x-vgr-content-sha256;x-vgr-date");
    let parsed = AuthorizationHeader::parse(&raw).unwrap();
    assert_eq!(parsed.access_key_id, "VGAKEXAMPLE7TESTKEY");
    assert_eq!(parsed.service, "speech");
    assert_eq!(parsed.to_string(), raw);
  }

  #[test]
  fn rejects_missing_required_header() {
    let err = AuthorizationHeader::parse(&header("host;x-vgr-date")).unwrap_err();
    assert_eq!(err.code(), "IncompleteSignature");
  }

  #[test]
  fn rejects_unsorted_signed_headers() {
    assert!(AuthorizationHeader::parse(&header("x-vgr-date;host;x-vgr-content-sha256")).is_err());
  }

  #[test]
  fn rejects_other_algorithms() {
    assert!(AuthorizationHeader::parse("AWS4-HMAC-SHA256 Credential=a/b/c/d").is_err());
    assert!(AuthorizationHeader::parse("Bearer abc").is_err());
  }
}

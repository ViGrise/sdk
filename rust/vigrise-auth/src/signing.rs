//! Key derivation, string-to-sign and signature primitives shared by clients,
//! services and the platform.

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::canonical::hex_sha256;
use crate::error::AuthError;

type HmacSha256 = Hmac<Sha256>;

pub const ALGORITHM: &str = "VGR4-HMAC-SHA256";
pub const TERMINATOR: &str = "vgr4_request";
pub const TIMESTAMP_FORMAT: &str = "%Y%m%dT%H%M%SZ";

pub fn scope(date: &str, service: &str) -> String {
  format!("{date}/{service}/{TERMINATOR}")
}

pub fn string_to_sign(timestamp: &str, scope: &str, canonical_request: &str) -> String {
  format!(
    "{ALGORITHM}\n{timestamp}\n{scope}\n{}",
    hex_sha256(canonical_request.as_bytes())
  )
}

pub(crate) fn hmac_sha256(key: &[u8], message: &str) -> [u8; 32] {
  let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts keys of any length");
  mac.update(message.as_bytes());
  mac.finalize().into_bytes().into()
}

pub fn derive_signing_key(secret_access_key: &str, date: &str, service: &str) -> [u8; 32] {
  let k_date = hmac_sha256(format!("VGR4{secret_access_key}").as_bytes(), date);
  let k_service = hmac_sha256(&k_date, service);
  hmac_sha256(&k_service, TERMINATOR)
}

pub fn signature(signing_key: &[u8; 32], string_to_sign: &str) -> String {
  hex::encode(hmac_sha256(signing_key, string_to_sign))
}

/// Constant-time comparison of a hex signature against the expected value.
pub fn verify_signature(signing_key: &[u8; 32], string_to_sign: &str, provided_hex: &str) -> bool {
  let Ok(provided) = hex::decode(provided_hex) else {
    return false;
  };
  let mut mac = HmacSha256::new_from_slice(signing_key).expect("HMAC accepts keys of any length");
  mac.update(string_to_sign.as_bytes());
  mac.verify_slice(&provided).is_ok()
}

/// The signature scheme a string to sign belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigningAlgorithm {
  Vgr4,
  Aws4,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedStringToSign {
  pub algorithm: SigningAlgorithm,
  pub timestamp: String,
  pub date: String,
  /// The credential scope region; present for AWS4 only.
  pub region: Option<String>,
  pub service: String,
  pub canonical_request_hash: String,
}

impl ParsedStringToSign {
  /// The signing key of this string's scope, derived with its scheme.
  pub fn signing_key(&self, secret_access_key: &str) -> [u8; 32] {
    match (&self.algorithm, &self.region) {
      (SigningAlgorithm::Aws4, Some(region)) => {
        crate::sigv4::derive_signing_key(secret_access_key, &self.date, region, &self.service)
      }
      _ => derive_signing_key(secret_access_key, &self.date, &self.service),
    }
  }
}

pub fn parse_string_to_sign(value: &str) -> Result<ParsedStringToSign, AuthError> {
  let invalid = || AuthError::IncompleteSignature("Malformed string to sign".to_string());
  let lines: Vec<&str> = value.split('\n').collect();
  let [algorithm, timestamp, scope, hash] = lines.as_slice() else {
    return Err(invalid());
  };
  let algorithm = match *algorithm {
    ALGORITHM => SigningAlgorithm::Vgr4,
    crate::sigv4::ALGORITHM => SigningAlgorithm::Aws4,
    _ => return Err(invalid()),
  };
  if !crate::sigv4::timestamp_shape_ok(timestamp)
    || chrono::NaiveDateTime::parse_from_str(timestamp, TIMESTAMP_FORMAT).is_err()
  {
    return Err(invalid());
  }
  let scope_parts: Vec<&str> = scope.split('/').collect();
  let (date, region, service, terminator) = match (algorithm, scope_parts.as_slice()) {
    (SigningAlgorithm::Vgr4, [date, service, terminator]) => (date, None, service, terminator),
    (SigningAlgorithm::Aws4, [date, region, service, terminator]) if !region.is_empty() => {
      (date, Some(region), service, terminator)
    }
    _ => return Err(invalid()),
  };
  let expected_terminator = match algorithm {
    SigningAlgorithm::Vgr4 => TERMINATOR,
    SigningAlgorithm::Aws4 => crate::sigv4::TERMINATOR,
  };
  if *terminator != expected_terminator || **date != timestamp[..8] || service.is_empty() {
    return Err(invalid());
  }
  if hash.len() != 64
    || !hash
      .bytes()
      .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
  {
    return Err(invalid());
  }
  Ok(ParsedStringToSign {
    algorithm,
    timestamp: timestamp.to_string(),
    date: date.to_string(),
    region: region.map(|r| r.to_string()),
    service: service.to_string(),
    canonical_request_hash: hash.to_string(),
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn verify_accepts_own_signature_and_rejects_others() {
    let key = derive_signing_key("secret", "20261001", "tts");
    let sig = signature(&key, "payload");
    assert!(verify_signature(&key, "payload", &sig));
    assert!(!verify_signature(&key, "payload2", &sig));
    assert!(!verify_signature(&key, "payload", "zz"));
  }

  #[test]
  fn parse_string_to_sign_round_trips() {
    let sts = string_to_sign("20261001T120000Z", &scope("20261001", "tts"), "req");
    let parsed = parse_string_to_sign(&sts).unwrap();
    assert_eq!(parsed.date, "20261001");
    assert_eq!(parsed.service, "tts");
  }

  #[test]
  fn parse_accepts_aws4_with_its_region_and_derives_the_matching_key() {
    let scope = crate::sigv4::Scope::new("20130524", "us-east-1", "s3");
    let sts = crate::sigv4::string_to_sign("20130524T000000Z", &scope, "req");
    let parsed = parse_string_to_sign(&sts).unwrap();
    assert_eq!(parsed.algorithm, SigningAlgorithm::Aws4);
    assert_eq!(parsed.region.as_deref(), Some("us-east-1"));
    assert_eq!(parsed.service, "s3");
    assert_eq!(
      parsed.signing_key("secret"),
      crate::sigv4::derive_signing_key("secret", "20130524", "us-east-1", "s3")
    );
    let vgr4 = parse_string_to_sign(&string_to_sign(
      "20261001T120000Z",
      &super::scope("20261001", "tts"),
      "req",
    ))
    .unwrap();
    assert_eq!(vgr4.algorithm, SigningAlgorithm::Vgr4);
    assert_eq!(vgr4.region, None);
    assert_eq!(
      vgr4.signing_key("secret"),
      derive_signing_key("secret", "20261001", "tts")
    );
  }

  #[test]
  fn parse_rejects_mixed_scope_shapes() {
    let hash = "a".repeat(64);
    for sts in [
      format!("AWS4-HMAC-SHA256\n20130524T000000Z\n20130524/s3/aws4_request\n{hash}"),
      format!("AWS4-HMAC-SHA256\n20130524T000000Z\n20130524/us-east-1/s3/vgr4_request\n{hash}"),
      format!("VGR4-HMAC-SHA256\n20130524T000000Z\n20130524/us-east-1/s3/vgr4_request\n{hash}"),
    ] {
      assert!(parse_string_to_sign(&sts).is_err(), "accepted {sts:?}");
    }
  }

  #[test]
  fn parse_rejects_partial_or_empty_scope_date() {
    for date in ["", "2026", "2026100"] {
      let sts = format!(
        "{ALGORITHM}\n20261001T120000Z\n{date}/tts/{TERMINATOR}\n{}",
        "a".repeat(64)
      );
      assert!(
        parse_string_to_sign(&sts).is_err(),
        "accepted scope date {date:?}"
      );
    }
  }

  #[test]
  fn parse_rejects_non_digit_timestamp() {
    let sts = format!(
      "{ALGORITHM}\n202610 1T120000Z\n202610 1/tts/{TERMINATOR}\n{}",
      "a".repeat(64)
    );
    assert!(parse_string_to_sign(&sts).is_err());
  }

  #[test]
  fn parse_rejects_scope_date_mismatch() {
    let sts = string_to_sign("20261001T120000Z", &scope("20261002", "tts"), "req");
    assert!(parse_string_to_sign(&sts).is_err());
  }
}

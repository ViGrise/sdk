use chrono::{DateTime, Utc};
use http::{header, HeaderMap, HeaderValue, Method, Uri};

use crate::authorization::AuthorizationHeader;
use crate::canonical::{canonical_request, hex_sha256};
use crate::credentials::Credentials;
use crate::error::AuthError;
use crate::signing::{derive_signing_key, scope, signature, string_to_sign, TIMESTAMP_FORMAT};

pub const X_VGR_DATE: &str = "x-vgr-date";
pub const X_VGR_CONTENT_SHA256: &str = "x-vgr-content-sha256";

/// Headers a client adds to a request to sign it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedRequestHeaders {
  pub authorization: String,
  pub x_vgr_date: String,
  pub x_vgr_content_sha256: String,
}

impl SignedRequestHeaders {
  pub fn apply(&self, headers: &mut HeaderMap) -> Result<(), AuthError> {
    let value = |v: &str| {
      HeaderValue::from_str(v)
        .map_err(|_| AuthError::IncompleteSignature("Invalid header value".into()))
    };
    headers.insert(header::AUTHORIZATION, value(&self.authorization)?);
    headers.insert(X_VGR_DATE, value(&self.x_vgr_date)?);
    headers.insert(X_VGR_CONTENT_SHA256, value(&self.x_vgr_content_sha256)?);
    Ok(())
  }
}

/// Host as an HTTP client sends it: the URI authority without a default port.
fn host_for(uri: &Uri) -> Result<String, AuthError> {
  let host = uri
    .host()
    .ok_or_else(|| AuthError::IncompleteSignature("Request URI has no host".into()))?;
  let default_port = match uri.scheme_str() {
    Some("http") => Some(80),
    Some("https") => Some(443),
    _ => None,
  };
  Ok(match uri.port_u16() {
    Some(port) if Some(port) != default_port => format!("{host}:{port}"),
    _ => host.to_string(),
  })
}

/// Signs `host`, `content-type` when present, and every `x-vgr-*` header.
pub fn sign_request(
  method: &Method,
  uri: &Uri,
  headers: &HeaderMap,
  body: &[u8],
  credentials: &Credentials,
  service: &str,
  time: DateTime<Utc>,
) -> Result<SignedRequestHeaders, AuthError> {
  let timestamp = time.format(TIMESTAMP_FORMAT).to_string();
  let date = timestamp[..8].to_string();
  let payload_hash = hex_sha256(body);

  let mut signing_headers = headers.clone();
  if !signing_headers.contains_key(header::HOST) {
    let host = HeaderValue::from_str(&host_for(uri)?)
      .map_err(|_| AuthError::IncompleteSignature("Invalid host".into()))?;
    signing_headers.insert(header::HOST, host);
  }
  signing_headers.insert(
    X_VGR_DATE,
    HeaderValue::from_str(&timestamp).expect("timestamp is ASCII"),
  );
  signing_headers.insert(
    X_VGR_CONTENT_SHA256,
    HeaderValue::from_str(&payload_hash).expect("hex is ASCII"),
  );

  let mut signed_headers: Vec<String> = signing_headers
    .keys()
    .map(|name| name.as_str().to_string())
    .filter(|name| name == "host" || name == "content-type" || name.starts_with("x-vgr-"))
    .collect();
  signed_headers.sort();
  signed_headers.dedup();

  let canonical = canonical_request(
    method.as_str(),
    uri.path(),
    uri.query().unwrap_or(""),
    &signing_headers,
    &signed_headers,
    &payload_hash,
  )?;
  let credential_scope = scope(&date, service);
  let sts = string_to_sign(&timestamp, &credential_scope, &canonical);
  let key = derive_signing_key(&credentials.secret_access_key, &date, service);

  let authorization = AuthorizationHeader {
    access_key_id: credentials.access_key_id.clone(),
    date,
    service: service.to_string(),
    signed_headers,
    signature: signature(&key, &sts),
  };
  Ok(SignedRequestHeaders {
    authorization: authorization.to_string(),
    x_vgr_date: timestamp,
    x_vgr_content_sha256: payload_hash,
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn host_drops_default_port_only() {
    assert_eq!(
      host_for(&"https://a.com:443/x".parse().unwrap()).unwrap(),
      "a.com"
    );
    assert_eq!(
      host_for(&"http://a.com:8080/x".parse().unwrap()).unwrap(),
      "a.com:8080"
    );
  }
}

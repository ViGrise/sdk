use chrono::{DateTime, Utc};

use crate::auth::credentials::Credentials;
use crate::auth::error::AuthError;
use crate::auth::signer::sign_request;

/// Signs a built `reqwest::Request` in place. Streaming bodies are rejected
/// because VGR4 signs the full payload hash.
pub fn sign_reqwest(
  request: &mut reqwest::Request,
  credentials: &Credentials,
  service: &str,
  time: DateTime<Utc>,
) -> Result<(), AuthError> {
  let body: &[u8] = match request.body() {
    None => &[],
    Some(body) => body.as_bytes().ok_or_else(|| {
      AuthError::IncompleteSignature("Streaming request bodies cannot be signed".into())
    })?,
  };
  let uri: http::Uri = request
    .url()
    .as_str()
    .parse()
    .map_err(|_| AuthError::IncompleteSignature("Request URL is not a valid URI".into()))?;
  let signed = sign_request(
    request.method(),
    &uri,
    request.headers(),
    body,
    credentials,
    service,
    time,
  )?;
  signed.apply(request.headers_mut())
}

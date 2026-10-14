//! Verification of webhooks sent by the Speech API, which follow Standard
//! Webhooks: HMAC-SHA256 over `<id>.<timestamp>.<body>` with the base64
//! decoded secret, sent as space-separated `v1,<base64>` entries.

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use http::HeaderMap;
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::speech::dto::WebhookEvent;

/// Prefix of a webhook secret as it is shown to the project.
pub const SECRET_PREFIX: &str = "whsec_";

/// How far a webhook's timestamp may be from the clock.
pub const DEFAULT_TOLERANCE: Duration = Duration::from_secs(5 * 60);

/// Why a webhook was refused.
#[derive(Debug, thiserror::Error)]
pub enum WebhookError {
  /// A `webhook-id`, `webhook-timestamp` or `webhook-signature` header is missing or unreadable.
  #[error("the {0} header is missing or unreadable")]
  MissingHeader(&'static str),
  /// The timestamp is not a number of seconds.
  #[error("the webhook timestamp is not a number")]
  InvalidTimestamp,
  /// The timestamp is further from the clock than the tolerance.
  #[error("the webhook timestamp is outside the tolerance")]
  TimestampOutOfTolerance,
  /// The secret is not base64.
  #[error("the webhook secret is not base64")]
  InvalidSecret,
  /// No signature of the header matches the body.
  #[error("no webhook signature matches")]
  SignatureMismatch,
  /// The signature is valid and the body is not a webhook event.
  #[error("the webhook body is not an event: {0}")]
  InvalidPayload(String),
}

fn header<'a>(headers: &'a HeaderMap, name: &'static str) -> Result<&'a str, WebhookError> {
  headers
    .get(name)
    .and_then(|value| value.to_str().ok())
    .ok_or(WebhookError::MissingHeader(name))
}

/// Checks the signature headers of a webhook against the raw body, at `now`.
pub fn verify_signature_at(
  secret: &str,
  headers: &HeaderMap,
  raw_body: &[u8],
  tolerance: Duration,
  now: DateTime<Utc>,
) -> Result<(), WebhookError> {
  let id = header(headers, "webhook-id")?;
  let timestamp = header(headers, "webhook-timestamp")?;
  let signatures = header(headers, "webhook-signature")?;
  let seconds: i64 = timestamp
    .parse()
    .map_err(|_| WebhookError::InvalidTimestamp)?;
  if now.timestamp().abs_diff(seconds) > tolerance.as_secs() {
    return Err(WebhookError::TimestampOutOfTolerance);
  }
  let key = STANDARD
    .decode(secret.strip_prefix(SECRET_PREFIX).unwrap_or(secret))
    .map_err(|_| WebhookError::InvalidSecret)?;
  let mut mac = Hmac::<Sha256>::new_from_slice(&key).map_err(|_| WebhookError::InvalidSecret)?;
  mac.update(id.as_bytes());
  mac.update(b".");
  mac.update(timestamp.as_bytes());
  mac.update(b".");
  mac.update(raw_body);
  let expected = mac.finalize().into_bytes();
  let matched = signatures
    .split_whitespace()
    .filter_map(|entry| entry.strip_prefix("v1,"))
    .filter_map(|encoded| STANDARD.decode(encoded).ok())
    .fold(false, |found, candidate| {
      found | bool::from(candidate.as_slice().ct_eq(&expected[..]))
    });
  if matched {
    Ok(())
  } else {
    Err(WebhookError::SignatureMismatch)
  }
}

/// Verifies a webhook and parses its event. `raw_body` is the request body as
/// received, before any JSON parsing.
pub fn verify_webhook_at(
  secret: &str,
  headers: &HeaderMap,
  raw_body: &[u8],
  tolerance: Duration,
  now: DateTime<Utc>,
) -> Result<WebhookEvent, WebhookError> {
  verify_signature_at(secret, headers, raw_body, tolerance, now)?;
  #[derive(serde::Deserialize)]
  struct Body {
    #[serde(rename = "type")]
    kind: String,
    timestamp: DateTime<Utc>,
    data: crate::speech::dto::Job,
  }
  let body: Body = serde_json::from_slice(raw_body)
    .map_err(|error| WebhookError::InvalidPayload(error.to_string()))?;
  Ok(WebhookEvent {
    id: header(headers, "webhook-id")?.to_string(),
    kind: body.kind,
    timestamp: body.timestamp,
    data: body.data,
  })
}

/// Verifies a webhook against the current time with the default tolerance of
/// five minutes.
pub fn verify_webhook(
  secret: &str,
  headers: &HeaderMap,
  raw_body: &[u8],
) -> Result<WebhookEvent, WebhookError> {
  verify_webhook_at(secret, headers, raw_body, DEFAULT_TOLERANCE, Utc::now())
}

/// Like [`verify_webhook`] with a tolerance of your own.
pub fn verify_webhook_within(
  secret: &str,
  headers: &HeaderMap,
  raw_body: &[u8],
  tolerance: Duration,
) -> Result<WebhookEvent, WebhookError> {
  verify_webhook_at(secret, headers, raw_body, tolerance, Utc::now())
}

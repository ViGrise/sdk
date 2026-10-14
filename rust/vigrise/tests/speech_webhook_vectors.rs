#![cfg(feature = "speech")]

use std::path::PathBuf;
use std::time::Duration;

use chrono::{TimeZone, Utc};
use http::{HeaderMap, HeaderValue};
use serde::Deserialize;
use vigrise::speech::webhook::{verify_signature_at, verify_webhook_at, WebhookError};

#[derive(Deserialize)]
struct Document {
  cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
  name: String,
  secret: String,
  id: String,
  timestamp: i64,
  body: String,
  signature: String,
  now: i64,
  tolerance_seconds: u64,
  valid: bool,
  error: Option<String>,
}

fn cases() -> Vec<Case> {
  let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .join("../../test-vectors/webhooks/standard-webhooks.json");
  serde_json::from_str::<Document>(&std::fs::read_to_string(path).unwrap())
    .unwrap()
    .cases
}

fn headers(case: &Case) -> HeaderMap {
  let mut headers = HeaderMap::new();
  headers.insert("webhook-id", HeaderValue::from_str(&case.id).unwrap());
  headers.insert(
    "webhook-timestamp",
    HeaderValue::from_str(&case.timestamp.to_string()).unwrap(),
  );
  headers.insert(
    "webhook-signature",
    HeaderValue::from_str(&case.signature).unwrap(),
  );
  headers
}

#[test]
fn every_vector_is_verified_as_expected() {
  let cases = cases();
  assert!(cases.len() >= 10);
  for case in cases {
    let now = Utc.timestamp_opt(case.now, 0).unwrap();
    let outcome = verify_signature_at(
      &case.secret,
      &headers(&case),
      case.body.as_bytes(),
      Duration::from_secs(case.tolerance_seconds),
      now,
    );
    match (case.valid, case.error.as_deref()) {
      (true, _) => assert!(outcome.is_ok(), "{}: {outcome:?}", case.name),
      (false, Some("signature")) => assert!(
        matches!(outcome, Err(WebhookError::SignatureMismatch)),
        "{}: {outcome:?}",
        case.name
      ),
      (false, Some("timestamp")) => assert!(
        matches!(outcome, Err(WebhookError::TimestampOutOfTolerance)),
        "{}: {outcome:?}",
        case.name
      ),
      (false, Some("secret")) => assert!(
        matches!(outcome, Err(WebhookError::InvalidSecret)),
        "{}: {outcome:?}",
        case.name
      ),
      other => panic!("{}: unexpected vector {other:?}", case.name),
    }
  }
}

#[test]
fn a_verified_webhook_becomes_an_event() {
  use base64::Engine;
  use hmac::{Hmac, Mac};
  use vigrise::speech::JobStatus;

  let secret = "whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw";
  let body = r#"{"type":"job.succeeded","timestamp":"2026-10-01T12:00:05Z","data":{"id":"job_1","object":"job","status":"succeeded","progress":{"pieces_done":2,"pieces_total":2,"characters_done":10,"characters":10},"characters_used":10,"format":"wav","webhook_url":"https://example.com/hook","error":null,"generation_id":"gen_1","created_by":"me","created_at":"2026-10-01T12:00:00Z","finished_at":"2026-10-01T12:00:05Z"}}"#;
  let now = Utc.with_ymd_and_hms(2026, 10, 1, 12, 0, 6).unwrap();
  let key = base64::engine::general_purpose::STANDARD
    .decode(&secret[6..])
    .unwrap();
  let mut mac = Hmac::<sha2::Sha256>::new_from_slice(&key).unwrap();
  mac.update(format!("evt_1.{}.{body}", now.timestamp()).as_bytes());
  let signature = format!(
    "v1,{}",
    base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes())
  );
  let mut headers = HeaderMap::new();
  headers.insert("webhook-id", HeaderValue::from_static("evt_1"));
  headers.insert(
    "webhook-timestamp",
    HeaderValue::from_str(&now.timestamp().to_string()).unwrap(),
  );
  headers.insert(
    "webhook-signature",
    HeaderValue::from_str(&signature).unwrap(),
  );

  let event = verify_webhook_at(
    secret,
    &headers,
    body.as_bytes(),
    Duration::from_secs(300),
    now,
  )
  .unwrap();
  assert_eq!(event.id, "evt_1");
  assert_eq!(event.kind, "job.succeeded");
  assert_eq!(event.data.status, JobStatus::Succeeded);
  assert_eq!(event.data.generation_id.as_deref(), Some("gen_1"));

  let tampered = body.replace("job_1", "job_2");
  assert!(matches!(
    verify_webhook_at(
      secret,
      &headers,
      tampered.as_bytes(),
      Duration::from_secs(300),
      now
    ),
    Err(WebhookError::SignatureMismatch)
  ));
}

#[test]
fn a_timestamp_at_the_edge_of_the_integer_range_is_out_of_tolerance() {
  let mut headers = HeaderMap::new();
  headers.insert("webhook-id", HeaderValue::from_static("evt_1"));
  headers.insert(
    "webhook-timestamp",
    HeaderValue::from_static("-9223372036854775808"),
  );
  headers.insert("webhook-signature", HeaderValue::from_static("v1,AAAA"));
  let outcome = verify_signature_at(
    "whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw",
    &headers,
    b"{}",
    Duration::from_secs(300),
    Utc.timestamp_opt(1_614_265_330, 0).unwrap(),
  );
  assert!(matches!(
    outcome,
    Err(WebhookError::TimestampOutOfTolerance)
  ));
}

#![cfg(feature = "metering")]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use uuid::Uuid;
use vigrise_auth::metering::{UsageBatch, UsageReport, UsageReporter, UsageReporterConfig};
use vigrise_auth::verify::{Authorized, Principal};
use vigrise_auth::Credentials;

#[derive(Clone, Default)]
struct Received {
  batches: Arc<Mutex<Vec<UsageBatch>>>,
  authorizations: Arc<Mutex<Vec<String>>>,
  statuses: Arc<Mutex<Vec<StatusCode>>>,
}

async fn usage(
  State(received): State<Received>,
  headers: HeaderMap,
  Json(batch): Json<UsageBatch>,
) -> (StatusCode, Json<UsageReport>) {
  received
    .authorizations
    .lock()
    .unwrap()
    .push(headers["authorization"].to_str().unwrap().to_string());
  let accepted = batch.records.len() as u64;
  received.batches.lock().unwrap().push(batch);
  let status = {
    let mut statuses = received.statuses.lock().unwrap();
    if statuses.is_empty() {
      StatusCode::OK
    } else {
      statuses.remove(0)
    }
  };
  (
    status,
    Json(UsageReport {
      accepted,
      duplicates: 0,
      rejected: vec![],
    }),
  )
}

async fn serve(statuses: Vec<StatusCode>) -> (String, Received) {
  let received = Received::default();
  *received.statuses.lock().unwrap() = statuses;
  let app = Router::new()
    .route("/usage", post(usage))
    .with_state(received.clone());
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let url = format!("http://{}/usage", listener.local_addr().unwrap());
  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  (url, received)
}

fn config(url: String) -> UsageReporterConfig {
  let mut config = UsageReporterConfig::new(url, Credentials::new("VGAKTESTKEY", "secret"), "tts");
  config.flush_interval = Duration::from_secs(3600);
  config.retry_backoff = Duration::from_millis(1);
  config.max_backoff = Duration::from_millis(5);
  config
}

fn authorized() -> Authorized {
  Authorized {
    principal: Principal {
      vrn: "vrn:vigrise:iam:p:user/ci-bot".into(),
      id: Uuid::from_u128(2),
      principal_type: "User".into(),
      name: "ci-bot".into(),
      project_id: Uuid::from_u128(1),
      organization_id: Uuid::from_u128(3),
    },
    tier: Some("Free".into()),
    quotas: vec![],
    usage: Default::default(),
    leases: vec![],
    signing_key: None,
  }
}

async fn wait_for_batches(received: &Received, count: usize) {
  for _ in 0..200 {
    if received.batches.lock().unwrap().len() >= count {
      return;
    }
    tokio::time::sleep(Duration::from_millis(10)).await;
  }
  panic!(
    "expected {count} batches, got {}",
    received.batches.lock().unwrap().len()
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn sends_a_batch_when_it_is_full_signed_with_the_metering_scope() {
  let (url, received) = serve(vec![]).await;
  let mut config = config(url);
  config.max_batch = 3;
  let reporter = UsageReporter::spawn(config);
  for quantity in 1..=3 {
    reporter.record(&authorized(), "SynthesizeSpeech", "characters", quantity);
  }
  wait_for_batches(&received, 1).await;
  let batch = received.batches.lock().unwrap()[0].clone();
  assert_eq!(batch.service, "tts");
  assert_eq!(
    batch.records.iter().map(|r| r.quantity).collect::<Vec<_>>(),
    vec![1, 2, 3]
  );
  assert_eq!(batch.records[0].project_id, Uuid::from_u128(1));
  assert_eq!(batch.records[0].principal_id, Some(Uuid::from_u128(2)));
  assert!(received.authorizations.lock().unwrap()[0].contains("/metering/vgr4_request"));
  reporter.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn sends_on_the_flush_interval() {
  let (url, received) = serve(vec![]).await;
  let mut config = config(url);
  config.flush_interval = Duration::from_millis(50);
  let reporter = UsageReporter::spawn(config);
  reporter.record(&authorized(), "SynthesizeSpeech", "characters", 5);
  wait_for_batches(&received, 1).await;
  reporter.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn server_errors_are_retried_with_the_same_record_ids() {
  let (url, received) = serve(vec![StatusCode::INTERNAL_SERVER_ERROR]).await;
  let reporter = UsageReporter::spawn(config(url));
  reporter.record(&authorized(), "SynthesizeSpeech", "characters", 5);
  reporter.shutdown().await;
  let batches = received.batches.lock().unwrap();
  assert_eq!(batches.len(), 2);
  assert_eq!(
    batches[0].records[0].record_id,
    batches[1].records[0].record_id
  );
}

#[tokio::test(flavor = "multi_thread")]
async fn client_errors_are_not_retried() {
  let (url, received) = serve(vec![StatusCode::BAD_REQUEST]).await;
  let reporter = UsageReporter::spawn(config(url));
  reporter.record(&authorized(), "SynthesizeSpeech", "characters", 5);
  reporter.shutdown().await;
  assert_eq!(received.batches.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_full_buffer_drops_records_and_shutdown_sends_the_rest() {
  let (url, received) = serve(vec![]).await;
  let mut config = config(url);
  config.capacity = 2;
  let reporter = UsageReporter::spawn(config);
  // The current-thread runtime runs the background task only at the next
  // await, so these five calls meet a buffer of two.
  for quantity in 1..=5 {
    reporter.record(&authorized(), "SynthesizeSpeech", "characters", quantity);
  }
  reporter.shutdown().await;
  let batches = received.batches.lock().unwrap();
  let quantities: Vec<i64> = batches
    .iter()
    .flat_map(|b| b.records.iter().map(|r| r.quantity))
    .collect();
  assert_eq!(quantities, vec![1, 2]);
}

#[tokio::test(flavor = "multi_thread")]
async fn transient_failures_are_retried_until_the_batch_is_delivered() {
  let (url, received) = serve(vec![StatusCode::SERVICE_UNAVAILABLE; 8]).await;
  let reporter = UsageReporter::spawn(config(url));
  reporter.record(&authorized(), "SynthesizeSpeech", "characters", 5);
  reporter.shutdown().await;
  let batches = received.batches.lock().unwrap();
  assert_eq!(batches.len(), 9);
  assert!(batches
    .iter()
    .all(|b| b.records[0].record_id == batches[0].records[0].record_id));
}

#[tokio::test(flavor = "multi_thread")]
async fn shutdown_gives_up_at_its_deadline() {
  let (url, _) = serve(vec![StatusCode::SERVICE_UNAVAILABLE; 100_000]).await;
  let mut config = config(url);
  config.shutdown_timeout = Duration::from_millis(200);
  let reporter = UsageReporter::spawn(config);
  reporter.record(&authorized(), "SynthesizeSpeech", "characters", 5);
  tokio::time::timeout(Duration::from_secs(2), reporter.shutdown())
    .await
    .expect("shutdown returns once its deadline passes");
}

/// Hands out `VGAKKEY<n>` and moves to the next key when invalidated, as a
/// role session provider does after a revocation.
#[derive(Debug, Default)]
struct Rotating {
  generation: std::sync::atomic::AtomicUsize,
}

impl vigrise_auth::CredentialsProvider for Rotating {
  fn credentials(&self) -> vigrise_auth::CredentialsFuture<'_> {
    let n = self.generation.load(std::sync::atomic::Ordering::SeqCst);
    Box::pin(async move { Ok(Credentials::new(format!("VGAKKEY{n}"), "secret")) })
  }

  fn invalidate(&self) -> bool {
    self
      .generation
      .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    true
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn refused_credentials_are_replaced_and_the_batch_is_retried() {
  let (url, received) = serve(vec![StatusCode::FORBIDDEN]).await;
  let mut config = config(url.clone());
  config.credentials = Arc::new(Rotating::default());
  config.max_batch = 1;
  let reporter = UsageReporter::spawn(config);
  reporter.record(&authorized(), "SynthesizeSpeech", "characters", 5);
  wait_for_batches(&received, 2).await;
  let authorizations = received.authorizations.lock().unwrap().clone();
  assert!(authorizations[0].contains("Credential=VGAKKEY0/"));
  assert!(authorizations[1].contains("Credential=VGAKKEY1/"));
  reporter.shutdown().await;
}

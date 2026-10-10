use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use tokio::sync::{mpsc, Notify};
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::types::{UsageBatch, UsageRecord, UsageReport};
use crate::provider::CredentialsProvider;
use crate::reqwest_signer::sign_reqwest;
use crate::verify::Authorized;

const SIGNING_SERVICE: &str = "metering";
/// Batches are retried while their oldest record is younger than this; the
/// platform accepts records up to 24 hours old.
const MAX_RETRY_AGE_HOURS: i64 = 23;

/// Settings of a [`UsageReporter`].
#[derive(Debug, Clone)]
pub struct UsageReporterConfig {
  /// Full URL, for example `https://platform.vigrise.com/api/internal/v1/usage`.
  pub endpoint: String,
  /// Credentials the batches are signed with.
  pub credentials: Arc<dyn CredentialsProvider>,
  /// Catalog namespace the usage belongs to.
  pub service: String,
  /// How often buffered records are sent.
  pub flush_interval: Duration,
  /// Most records in one batch.
  pub max_batch: usize,
  /// Records waiting to be sent; further records are dropped.
  pub capacity: usize,
  /// First delay between retries of a batch; it doubles up to `max_backoff`.
  pub retry_backoff: Duration,
  /// Longest delay between retries of a batch.
  pub max_backoff: Duration,
  /// How long `shutdown` waits for buffered usage to be sent.
  pub shutdown_timeout: Duration,
}

impl UsageReporterConfig {
  /// Creates a configuration with default batching and retry settings.
  pub fn new(
    endpoint: impl Into<String>,
    credentials: impl CredentialsProvider + 'static,
    service: impl Into<String>,
  ) -> Self {
    Self {
      endpoint: endpoint.into(),
      credentials: Arc::new(credentials),
      service: service.into(),
      flush_interval: Duration::from_secs(5),
      max_batch: 500,
      capacity: 10_000,
      retry_backoff: Duration::from_millis(200),
      max_backoff: Duration::from_secs(30),
      shutdown_timeout: Duration::from_secs(10),
    }
  }
}

/// Buffers usage records and sends them to the platform in the background, so
/// request latency never depends on the platform.
#[derive(Clone)]
pub struct UsageReporter {
  sender: mpsc::Sender<UsageRecord>,
  shutdown_timeout: Duration,
  shutdown: Arc<Notify>,
  task: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl UsageReporter {
  /// Starts the background task that sends records. Must run inside a Tokio runtime.
  pub fn spawn(config: UsageReporterConfig) -> Self {
    let (sender, receiver) = mpsc::channel(config.capacity.max(1));
    let shutdown_timeout = config.shutdown_timeout;
    let shutdown = Arc::new(Notify::new());
    let task = tokio::spawn(run(config, receiver, shutdown.clone()));
    Self {
      sender,
      shutdown_timeout,
      shutdown,
      task: Arc::new(Mutex::new(Some(task))),
    }
  }

  /// Buffers one usage record for the project and principal of `authorized`.
  /// The record is dropped when the buffer is full.
  pub fn record(&self, authorized: &Authorized, action: &str, dimension: &str, quantity: i64) {
    let record = UsageRecord {
      record_id: Uuid::new_v4(),
      project_id: authorized.principal.project_id,
      principal_id: Some(authorized.principal.id),
      action: action.to_string(),
      dimension: dimension.to_string(),
      quantity,
      occurred_at: Utc::now(),
    };
    if let Err(e) = self.sender.try_send(record) {
      tracing::error!(error = %e, action, dimension, quantity, "usage record dropped");
    }
  }

  /// Sends every buffered record, then stops the background task. Gives up
  /// after `shutdown_timeout`, so an unreachable platform cannot block exit.
  pub async fn shutdown(&self) {
    self.shutdown.notify_one();
    let task = self.task.lock().expect("reporter task lock").take();
    let Some(mut task) = task else {
      return;
    };
    match tokio::time::timeout(self.shutdown_timeout, &mut task).await {
      Ok(Ok(())) => {}
      Ok(Err(e)) => tracing::error!(error = %e, "usage reporter task failed"),
      Err(_) => {
        task.abort();
        tracing::error!("usage reporter stopped before sending all buffered usage");
      }
    }
  }
}

async fn run(
  config: UsageReporterConfig,
  mut receiver: mpsc::Receiver<UsageRecord>,
  shutdown: Arc<Notify>,
) {
  let http = reqwest::Client::builder()
    .timeout(Duration::from_secs(10))
    .build()
    .expect("reqwest client builds with static configuration");
  let mut buffer: Vec<UsageRecord> = Vec::new();
  let mut tick = tokio::time::interval(config.flush_interval);
  tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
  tick.tick().await;
  loop {
    tokio::select! {
      received = receiver.recv() => match received {
        Some(record) => {
          buffer.push(record);
          if buffer.len() >= config.max_batch {
            send(&http, &config, std::mem::take(&mut buffer)).await;
          }
        }
        None => break,
      },
      _ = tick.tick() => {
        if !buffer.is_empty() {
          send(&http, &config, std::mem::take(&mut buffer)).await;
        }
      }
      _ = shutdown.notified() => break,
    }
  }
  receiver.close();
  while let Ok(record) = receiver.try_recv() {
    buffer.push(record);
  }
  for chunk in buffer.chunks(config.max_batch.max(1)) {
    send(&http, &config, chunk.to_vec()).await;
  }
}

/// Retries network errors and server errors with the same record ids until the
/// batch is delivered or its oldest record is about to leave the window the
/// platform accepts. Refused batches are logged and dropped.
async fn send(http: &reqwest::Client, config: &UsageReporterConfig, records: Vec<UsageRecord>) {
  let count = records.len();
  let oldest = records.iter().map(|r| r.occurred_at).min();
  let batch = UsageBatch {
    service: config.service.clone(),
    records,
  };
  let mut backoff = config.retry_backoff;
  loop {
    match attempt_send(http, config, &batch).await {
      Ok(report) => {
        for rejected in report.rejected {
          tracing::warn!(record_id = %rejected.record_id, error = %rejected.error, "usage record rejected");
        }
        return;
      }
      Err(Failure::Permanent(reason)) => {
        tracing::error!(%reason, count, "usage batch refused");
        return;
      }
      Err(Failure::Transient(reason)) => {
        let expired =
          oldest.is_some_and(|t| Utc::now() - t > chrono::Duration::hours(MAX_RETRY_AGE_HOURS));
        if expired {
          tracing::error!(%reason, count, "usage batch dropped: its records are too old to report");
          return;
        }
        tracing::warn!(%reason, count, "usage batch failed, retrying");
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(config.max_backoff);
      }
    }
  }
}

enum Failure {
  Transient(String),
  Permanent(String),
}

async fn attempt_send(
  http: &reqwest::Client,
  config: &UsageReporterConfig,
  batch: &UsageBatch,
) -> Result<UsageReport, Failure> {
  let credentials = config
    .credentials
    .credentials()
    .await
    .map_err(|e| Failure::Transient(format!("service credentials are unavailable: {e}")))?;
  let mut request = http
    .post(&config.endpoint)
    .json(batch)
    .build()
    .map_err(|e| Failure::Permanent(e.to_string()))?;
  sign_reqwest(&mut request, &credentials, SIGNING_SERVICE, Utc::now())
    .map_err(|e| Failure::Permanent(e.to_string()))?;
  let response = http
    .execute(request)
    .await
    .map_err(|e| Failure::Transient(e.to_string()))?;
  let status = response.status();
  if status.is_server_error() {
    return Err(Failure::Transient(format!("platform returned {status}")));
  }
  let refused =
    status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN;
  if refused && config.credentials.invalidate() {
    return Err(Failure::Transient(format!(
      "platform refused the service's credentials with {status}"
    )));
  }
  if !status.is_success() {
    let body = response.text().await.unwrap_or_default();
    return Err(Failure::Permanent(format!(
      "platform returned {status}: {body}"
    )));
  }
  response
    .json::<UsageReport>()
    .await
    .map_err(|e| Failure::Permanent(format!("invalid usage response: {e}")))
}

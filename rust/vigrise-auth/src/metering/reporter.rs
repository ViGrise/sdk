use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use tokio::sync::{mpsc, Notify};
use tokio::task::JoinHandle;
use uuid::Uuid;

use super::types::{UsageBatch, UsageRecord, UsageReport};
use crate::credentials::Credentials;
use crate::reqwest_signer::sign_reqwest;
use crate::verify::Authorized;

const SIGNING_SERVICE: &str = "metering";

#[derive(Debug, Clone)]
pub struct UsageReporterConfig {
  /// Full URL, for example `https://platform.vigrise.com/api/internal/v1/usage`.
  pub endpoint: String,
  pub credentials: Credentials,
  /// Catalog namespace the usage belongs to.
  pub service: String,
  pub flush_interval: Duration,
  pub max_batch: usize,
  /// Records waiting to be sent; further records are dropped.
  pub capacity: usize,
  pub retry_backoff: Duration,
  pub max_attempts: u32,
}

impl UsageReporterConfig {
  pub fn new(endpoint: impl Into<String>, credentials: Credentials, service: impl Into<String>) -> Self {
    Self {
      endpoint: endpoint.into(),
      credentials,
      service: service.into(),
      flush_interval: Duration::from_secs(5),
      max_batch: 500,
      capacity: 10_000,
      retry_backoff: Duration::from_millis(200),
      max_attempts: 5,
    }
  }
}

/// Buffers usage records and sends them to the platform in the background, so
/// request latency never depends on the platform.
#[derive(Clone)]
pub struct UsageReporter {
  sender: mpsc::Sender<UsageRecord>,
  shutdown: Arc<Notify>,
  task: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl UsageReporter {
  pub fn spawn(config: UsageReporterConfig) -> Self {
    let (sender, receiver) = mpsc::channel(config.capacity.max(1));
    let shutdown = Arc::new(Notify::new());
    let task = tokio::spawn(run(config, receiver, shutdown.clone()));
    Self {
      sender,
      shutdown,
      task: Arc::new(Mutex::new(Some(task))),
    }
  }

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

  /// Sends every buffered record, then stops the background task.
  pub async fn shutdown(&self) {
    self.shutdown.notify_one();
    let task = self.task.lock().expect("reporter task lock").take();
    if let Some(task) = task {
      if let Err(e) = task.await {
        tracing::error!(error = %e, "usage reporter task failed");
      }
    }
  }
}

async fn run(config: UsageReporterConfig, mut receiver: mpsc::Receiver<UsageRecord>, shutdown: Arc<Notify>) {
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

async fn send(http: &reqwest::Client, config: &UsageReporterConfig, records: Vec<UsageRecord>) {
  let count = records.len();
  let batch = UsageBatch {
    service: config.service.clone(),
    records,
  };
  let mut backoff = config.retry_backoff;
  for attempt in 1..=config.max_attempts.max(1) {
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
        tracing::warn!(%reason, attempt, "usage batch failed");
        if attempt < config.max_attempts {
          tokio::time::sleep(backoff).await;
          backoff *= 2;
        }
      }
    }
  }
  tracing::error!(count, "usage batch dropped after retries");
}

enum Failure {
  Transient(String),
  Permanent(String),
}

async fn attempt_send(http: &reqwest::Client, config: &UsageReporterConfig, batch: &UsageBatch) -> Result<UsageReport, Failure> {
  let mut request = http
    .post(&config.endpoint)
    .json(batch)
    .build()
    .map_err(|e| Failure::Permanent(e.to_string()))?;
  sign_reqwest(&mut request, &config.credentials, SIGNING_SERVICE, Utc::now())
    .map_err(|e| Failure::Permanent(e.to_string()))?;
  let response = http
    .execute(request)
    .await
    .map_err(|e| Failure::Transient(e.to_string()))?;
  let status = response.status();
  if status.is_server_error() {
    return Err(Failure::Transient(format!("platform returned {status}")));
  }
  if !status.is_success() {
    let body = response.text().await.unwrap_or_default();
    return Err(Failure::Permanent(format!("platform returned {status}: {body}")));
  }
  response
    .json::<UsageReport>()
    .await
    .map_err(|e| Failure::Permanent(format!("invalid usage response: {e}")))
}

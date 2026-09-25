use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Body of `POST /internal/v1/usage`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageBatch {
  pub service: String,
  pub records: Vec<UsageRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageRecord {
  /// Idempotency key: the platform counts each id once per service.
  pub record_id: Uuid,
  pub project_id: Uuid,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub principal_id: Option<Uuid>,
  pub action: String,
  pub dimension: String,
  pub quantity: i64,
  pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageReport {
  pub accepted: u64,
  pub duplicates: u64,
  pub rejected: Vec<RejectedRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RejectedRecord {
  pub record_id: Uuid,
  pub error: String,
}

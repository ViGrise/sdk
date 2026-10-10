use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Body of `POST /internal/v1/usage`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UsageBatch {
  /// Catalog namespace the usage belongs to.
  pub service: String,
  /// The usage records of the batch.
  pub records: Vec<UsageRecord>,
}

/// One metered quantity of usage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UsageRecord {
  /// Idempotency key: the platform counts each id once per service.
  pub record_id: Uuid,
  /// Project that used the service.
  pub project_id: Uuid,
  /// Principal that made the request, when known.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub principal_id: Option<Uuid>,
  /// Action that consumed the usage.
  pub action: String,
  /// Metered dimension, such as characters or bytes.
  pub dimension: String,
  /// Amount consumed.
  pub quantity: i64,
  /// When the usage happened.
  pub occurred_at: DateTime<Utc>,
}

/// The platform's answer to a usage batch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UsageReport {
  /// Records counted.
  pub accepted: u64,
  /// Records that were already counted.
  pub duplicates: u64,
  /// Records refused.
  pub rejected: Vec<RejectedRecord>,
}

/// A record the platform refused.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RejectedRecord {
  /// The refused record.
  pub record_id: Uuid,
  /// Why the record was refused.
  pub error: String,
}

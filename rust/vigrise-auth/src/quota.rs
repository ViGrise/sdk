use serde::{Deserialize, Serialize};

/// A quota the platform checked while authorizing a request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotaStatus {
  pub service: String,
  pub code: String,
  /// `None` means unlimited.
  pub value: Option<i64>,
  /// Usage in the current period, or the current count for resource quotas.
  pub used: i64,
}

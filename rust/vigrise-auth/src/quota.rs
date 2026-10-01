use serde::{Deserialize, Serialize};

/// A quota the platform checked while authorizing a request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct QuotaStatus {
  pub service: String,
  pub code: String,
  /// `None` means unlimited.
  pub value: Option<i64>,
  /// Usage in the current period, or the current count for resource quotas.
  pub used: i64,
  /// Whether the quota refuses requests once `used` reaches `value`; false
  /// when use beyond `value` is billed.
  #[serde(default, skip_serializing_if = "is_false")]
  pub blocking: bool,
}

fn is_false(value: &bool) -> bool {
  !value
}

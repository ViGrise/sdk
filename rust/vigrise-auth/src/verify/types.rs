use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

pub use crate::QuotaStatus;

/// Body of `POST /internal/v1/authorize`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AuthorizeRequest {
  pub access_key_id: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub security_token: Option<String>,
  pub string_to_sign: String,
  pub signature: String,
  pub action: String,
  pub resources: Vec<String>,
  pub context: Map<String, Value>,
  /// Usage the request is expected to consume, per catalog usage dimension.
  #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
  pub usage: BTreeMap<String, i64>,
  /// Asks the platform to return the SigV4 signing key of the request's
  /// scope on an allow, for services that verify signed chunks themselves.
  #[serde(default, skip_serializing_if = "is_false")]
  pub streaming_signing_key: bool,
  /// Validity in seconds of a presigned request, counted from its signing time.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub presigned_expires: Option<i64>,
}

fn is_false(value: &bool) -> bool {
  !value
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum Decision {
  Allow,
  Deny,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ErrorBody {
  pub code: String,
  pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Principal {
  pub vrn: String,
  pub id: Uuid,
  #[serde(rename = "type")]
  pub principal_type: String,
  pub name: String,
  pub project_id: Uuid,
  pub organization_id: Uuid,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AuthorizeResponse {
  pub decision: Decision,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub error: Option<ErrorBody>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub principal: Option<Principal>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub tier: Option<String>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub quotas: Vec<QuotaStatus>,
  /// Concurrency slots held for this request; released when it ends.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub leases: Vec<String>,
  /// Hex SigV4 signing key derived for the request's date, region and
  /// service; returned only when the request asked for it.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub signing_key: Option<String>,
}

impl fmt::Debug for AuthorizeResponse {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("AuthorizeResponse")
      .field("decision", &self.decision)
      .field("error", &self.error)
      .field("principal", &self.principal)
      .field("tier", &self.tier)
      .field("quotas", &self.quotas)
      .field("leases", &self.leases)
      .field(
        "signing_key",
        &self.signing_key.as_ref().map(|_| "<redacted>"),
      )
      .finish()
  }
}

/// Inserted into request extensions for handlers behind the verify middleware.
#[derive(Clone, PartialEq, Eq)]
pub struct Authorized {
  pub principal: Principal,
  pub tier: Option<String>,
  /// Quotas checked for this request, for services to expose limits to callers.
  pub quotas: Vec<QuotaStatus>,
  /// The usage estimate the request was authorized for, per usage dimension.
  pub usage: BTreeMap<String, i64>,
  /// Concurrency slots held for this request; the middleware releases them
  /// when the response body ends or is dropped.
  pub leases: Vec<String>,
  /// Hex SigV4 signing key of the request's scope, present only when the
  /// service asked for it and the platform allowed it.
  pub signing_key: Option<String>,
}

impl fmt::Debug for Authorized {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Authorized")
      .field("principal", &self.principal)
      .field("tier", &self.tier)
      .field("quotas", &self.quotas)
      .field("usage", &self.usage)
      .field("leases", &self.leases)
      .field(
        "signing_key",
        &self.signing_key.as_ref().map(|_| "<redacted>"),
      )
      .finish()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn debug_output_redacts_the_signing_key() {
    let response = AuthorizeResponse {
      decision: Decision::Allow,
      error: None,
      principal: None,
      tier: None,
      quotas: vec![],
      leases: vec![],
      signing_key: Some("00ff00ff".into()),
    };
    let text = format!("{response:?}");
    assert!(!text.contains("00ff00ff"), "{text}");
    assert!(text.contains("<redacted>"));
  }

  #[test]
  fn the_request_flag_is_omitted_when_false() {
    let request: AuthorizeRequest = serde_json::from_value(serde_json::json!({
      "access_key_id": "k", "string_to_sign": "s", "signature": "g", "action": "a:B",
      "resources": [], "context": {}
    }))
    .unwrap();
    assert!(!request.streaming_signing_key);
    let json = serde_json::to_value(&request).unwrap();
    assert!(json.get("streaming_signing_key").is_none());
    assert!(json.get("presigned_expires").is_none());
  }
}

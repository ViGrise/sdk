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
  /// Access key ID of the signer.
  pub access_key_id: String,
  /// Session token of a role session.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub security_token: Option<String>,
  /// The string to sign the request was signed over.
  pub string_to_sign: String,
  /// Hex-encoded signature of the request.
  pub signature: String,
  /// Action the request asks to perform, `<namespace>:<Action>`.
  pub action: String,
  /// VRNs the action applies to.
  pub resources: Vec<String>,
  /// Condition keys the policies may test.
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

/// The outcome of an authorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum Decision {
  /// The request is allowed.
  Allow,
  /// The request is denied.
  Deny,
}

/// The reason an authorization was refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ErrorBody {
  /// Wire error code.
  pub code: String,
  /// Human-readable message.
  pub message: String,
}

/// The authenticated caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Principal {
  /// VRN of the principal.
  pub vrn: String,
  /// ID of the principal.
  pub id: Uuid,
  /// Kind of principal, such as a user or a role session.
  #[serde(rename = "type")]
  pub principal_type: String,
  /// Name of the principal.
  pub name: String,
  /// Project the principal acts in.
  pub project_id: Uuid,
  /// Organization that owns the project.
  pub organization_id: Uuid,
  /// The source identity a role session was started with; absent for other
  /// principals and for sessions that set none.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub source_identity: Option<String>,
  /// The VRN of the principal that assumed the role, as recorded in the
  /// session; absent for principals that are not role sessions.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub source_principal: Option<String>,
}

/// The platform's answer to an authorization request.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct AuthorizeResponse {
  /// Whether the request is allowed.
  pub decision: Decision,
  /// Why the request is denied.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub error: Option<ErrorBody>,
  /// The caller, present on an allow.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub principal: Option<Principal>,
  /// Subscription tier of the project.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub tier: Option<String>,
  /// Quotas that were checked.
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
  /// The authenticated caller.
  pub principal: Principal,
  /// Subscription tier of the project.
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
  fn a_principal_without_a_source_identity_reads_as_none() {
    let principal: Principal = serde_json::from_value(serde_json::json!({
      "vrn": "vrn:vigrise:iam:p:user/a", "id": Uuid::nil(), "type": "user", "name": "a",
      "project_id": Uuid::nil(), "organization_id": Uuid::nil()
    }))
    .unwrap();
    assert_eq!(principal.source_identity, None);
    let json = serde_json::to_value(&principal).unwrap();
    assert!(json.get("source_identity").is_none());
    let with: Principal = serde_json::from_value(serde_json::json!({
      "vrn": "v", "id": Uuid::nil(), "type": "role-session", "name": "a",
      "project_id": Uuid::nil(), "organization_id": Uuid::nil(), "source_identity": "alice"
    }))
    .unwrap();
    assert_eq!(with.source_identity.as_deref(), Some("alice"));
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

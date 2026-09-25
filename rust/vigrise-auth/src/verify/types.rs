use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use uuid::Uuid;

pub use crate::QuotaStatus;

/// Body of `POST /internal/v1/authorize`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuthorizeRequest {
  pub access_key_id: String,
  pub string_to_sign: String,
  pub signature: String,
  pub action: String,
  pub resources: Vec<String>,
  pub context: Map<String, Value>,
  /// Usage the request is expected to consume, per catalog usage dimension.
  #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
  pub usage: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Decision {
  Allow,
  Deny,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorBody {
  pub code: String,
  pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Principal {
  pub vrn: String,
  pub id: Uuid,
  #[serde(rename = "type")]
  pub principal_type: String,
  pub name: String,
  pub project_id: Uuid,
  pub organization_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizeResponse {
  pub decision: Decision,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub error: Option<ErrorBody>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub principal: Option<Principal>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub project_plan: Option<String>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub quotas: Vec<QuotaStatus>,
  /// Concurrency slots held for this request; released when it ends.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub leases: Vec<String>,
}

/// Inserted into request extensions for handlers behind the verify middleware.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authorized {
  pub principal: Principal,
  pub project_plan: Option<String>,
  /// Quotas checked for this request, for services to expose limits to callers.
  pub quotas: Vec<QuotaStatus>,
  /// The usage estimate the request was authorized for, per usage dimension.
  pub usage: BTreeMap<String, i64>,
  /// Concurrency slots held for this request; the middleware releases them
  /// when the response body ends or is dropped.
  pub leases: Vec<String>,
}

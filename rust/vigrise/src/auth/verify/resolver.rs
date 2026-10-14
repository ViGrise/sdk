use std::collections::BTreeMap;

use bytes::Bytes;
use http::request::Parts;
use serde_json::{Map, Value};

use crate::auth::error::AuthError;

/// What a request asks to do, in catalog terms.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResolvedAction {
  /// `<namespace>:<Action>`, for example `speech:SynthesizeSpeech`.
  pub action: String,
  /// VRNs; `${ProjectId}` is substituted by the platform.
  pub resources: Vec<String>,
  /// Service-specific condition keys declared in the catalog.
  pub context: Map<String, Value>,
  /// Usage estimate per catalog usage dimension, checked against quotas.
  pub usage: BTreeMap<String, i64>,
}

/// Maps an incoming request to its catalog action. Returning an error rejects
/// the request before the platform is called; unknown routes must be rejected.
pub trait ActionResolver: Send + Sync + 'static {
  /// Resolves the action of a request from its head and body.
  fn resolve(&self, parts: &Parts, body: &Bytes) -> Result<ResolvedAction, AuthError>;

  /// Further actions the caller must be allowed to perform for the same
  /// request, such as reading the source of a copy. Each is authorized with
  /// the request's own signature; a denial rejects the request.
  fn additional(&self, parts: &Parts, body: &Bytes) -> Result<Vec<ResolvedAction>, AuthError> {
    let _ = (parts, body);
    Ok(Vec::new())
  }
}

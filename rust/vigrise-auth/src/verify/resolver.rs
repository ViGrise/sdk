use bytes::Bytes;
use http::request::Parts;
use serde_json::{Map, Value};

use crate::error::AuthError;

/// What a request asks to do, in catalog terms.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResolvedAction {
  /// `<namespace>:<Action>`, for example `tts:SynthesizeSpeech`.
  pub action: String,
  /// VRNs; `${ProjectId}` is substituted by the platform.
  pub resources: Vec<String>,
  /// Service-specific condition keys declared in the catalog.
  pub context: Map<String, Value>,
}

/// Maps an incoming request to its catalog action. Returning an error rejects
/// the request before the platform is called; unknown routes must be rejected.
pub trait ActionResolver: Send + Sync + 'static {
  fn resolve(&self, parts: &Parts, body: &Bytes) -> Result<ResolvedAction, AuthError>;
}

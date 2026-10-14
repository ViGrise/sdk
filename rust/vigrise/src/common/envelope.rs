//! The error envelope of the service APIs: `{"error": {"code", "message", ...}}`.

use serde_json::{Map, Value};

/// The fields of an error envelope.
// Each service uses part of this shared code.
#[cfg_attr(not(all(feature = "speech", feature = "storage")), allow(dead_code))]
pub(crate) struct ErrorEnvelope {
  pub code: Option<String>,
  pub message: String,
  /// The resource an answer refers to.
  pub resource_id: Option<String>,
  /// The fields besides `code`, `message` and `resource_id`.
  pub details: Map<String, Value>,
}

/// The envelope a response body carries; `None` when the body is not one.
pub(crate) fn parse_error_envelope(body: &[u8]) -> Option<ErrorEnvelope> {
  let Value::Object(mut root) = serde_json::from_slice::<Value>(body).ok()? else {
    return None;
  };
  let Some(Value::Object(mut error)) = root.remove("error") else {
    return None;
  };
  let mut text = |name: &str| match error.remove(name) {
    Some(Value::String(text)) => Some(text),
    _ => None,
  };
  let code = text("code");
  let message = text("message");
  let resource_id = text("resource_id");
  Some(ErrorEnvelope {
    code,
    message: message.unwrap_or_default(),
    resource_id,
    details: error,
  })
}

use serde_json::{Map, Value};

use crate::speech::types::Job;

/// Errors of Speech calls.
#[derive(Debug, thiserror::Error)]
pub enum SpeechError {
  /// The service answered with an error, such as `VoiceNotFound`.
  #[error("{code} ({status}): {message}")]
  Service {
    /// HTTP status of the answer.
    status: u16,
    /// Error code of the service, such as `VoiceNotFound`.
    code: String,
    /// Human-readable message.
    message: String,
    /// The resource an answer refers to, such as the generation an idempotency key already created.
    resource_id: Option<String>,
    /// The other fields of the error, such as `quota` on limit refusals.
    details: Map<String, Value>,
  },
  /// The request could not be sent, or the answer could not be read.
  #[error("request failed: {0}")]
  Transport(String),
  /// The request timed out.
  #[error("request timed out: {0}")]
  Timeout(String),
  /// The request could not be signed.
  #[error("signing failed: {0}")]
  Signing(String),
  /// A job did not finish within the time given to `wait_for_job`.
  #[error("job {} did not finish in time", last_job.id)]
  WaitTimeout {
    /// The job as it was at the last poll.
    last_job: Box<Job>,
  },
}

impl SpeechError {
  /// The service's error code, when the service answered with one.
  pub fn code(&self) -> Option<&str> {
    match self {
      Self::Service { code, .. } => Some(code),
      _ => None,
    }
  }

  /// The HTTP status, when the service answered.
  pub fn status(&self) -> Option<u16> {
    match self {
      Self::Service { status, .. } => Some(*status),
      _ => None,
    }
  }

  pub(crate) fn from_transport(error: reqwest::Error) -> Self {
    if error.is_timeout() {
      Self::Timeout(error.to_string())
    } else {
      Self::Transport(error.to_string())
    }
  }
}

/// The error an error response carries: `{"error": {"code", "message", ...}}`,
/// or the status when the body is not that envelope.
pub(crate) fn from_response(status: u16, body: &[u8]) -> SpeechError {
  let parsed = serde_json::from_slice::<Value>(body)
    .ok()
    .and_then(|value| match value {
      Value::Object(mut root) => match root.remove("error") {
        Some(Value::Object(error)) => Some(error),
        _ => None,
      },
      _ => None,
    });
  match parsed {
    Some(mut error) => {
      let take = |error: &mut Map<String, Value>, name: &str| match error.remove(name) {
        Some(Value::String(text)) => Some(text),
        _ => None,
      };
      let code = take(&mut error, "code").unwrap_or_else(|| "UnknownError".into());
      let message = take(&mut error, "message").unwrap_or_default();
      let resource_id = take(&mut error, "resource_id");
      SpeechError::Service {
        status,
        code,
        message,
        resource_id,
        details: error,
      }
    }
    None => SpeechError::Service {
      status,
      code: "UnknownError".into(),
      message: String::from_utf8_lossy(body).into_owned(),
      resource_id: None,
      details: Map::new(),
    },
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn error_envelopes_carry_their_extra_fields() {
    let error = from_response(
      409,
      br#"{"error":{"code":"IdempotencyReplayed","message":"ran","resource_id":"gen_1","quota":{"limit":3}}}"#,
    );
    match error {
      SpeechError::Service {
        status,
        code,
        resource_id,
        details,
        ..
      } => {
        assert_eq!((status, code.as_str()), (409, "IdempotencyReplayed"));
        assert_eq!(resource_id.as_deref(), Some("gen_1"));
        assert_eq!(details["quota"]["limit"], 3);
      }
      other => panic!("{other:?}"),
    }
  }

  #[test]
  fn other_bodies_become_unknown_errors() {
    assert_eq!(
      from_response(502, b"bad gateway").code(),
      Some("UnknownError")
    );
  }
}

use crate::common::envelope::{ErrorEnvelope, parse_error_envelope};
use crate::common::signed::SendError;

/// Errors of Storage calls.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
  /// The service answered with an error, such as `NoSuchKey`.
  #[error("{code} ({status}): {message}")]
  Service {
    /// HTTP status of the answer.
    status: u16,
    /// Error code of the service, such as `NoSuchKey`.
    code: String,
    /// Human-readable message.
    message: String,
  },
  /// The request could not be sent, or the answer could not be read.
  #[error("request failed: {0}")]
  Transport(String),
  /// The request could not be signed.
  #[error("signing failed: {0}")]
  Signing(String),
}

impl StorageError {
  /// The service's error code, when the service answered with one.
  pub fn code(&self) -> Option<&str> {
    match self {
      Self::Service { code, .. } => Some(code),
      _ => None,
    }
  }
}

/// The error an error response carries: `{"error": {"code", "message"}}`, or
/// the status when the body is not that envelope.
pub(crate) fn from_response(status: u16, body: &[u8]) -> StorageError {
  match parse_error_envelope(body) {
    Some(ErrorEnvelope {
      code: Some(code),
      message,
      ..
    }) => StorageError::Service {
      status,
      code,
      message,
    },
    _ => StorageError::Service {
      status,
      code: "UnknownError".into(),
      message: String::from_utf8_lossy(body).into_owned(),
    },
  }
}

impl From<SendError> for StorageError {
  fn from(error: SendError) -> Self {
    match error {
      SendError::InvalidUrl(message) => Self::Transport(message),
      SendError::Signing(message) => Self::Signing(message),
      SendError::Transport(error) => Self::Transport(error.to_string()),
      SendError::Refused { status, body } => from_response(status, &body),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn error_envelopes_become_service_errors() {
    let error = from_response(404, br#"{"error":{"code":"NoSuchKey","message":"gone"}}"#);
    assert_eq!(error.code(), Some("NoSuchKey"));
    let other = from_response(502, b"bad gateway");
    assert_eq!(other.code(), Some("UnknownError"));
  }
}

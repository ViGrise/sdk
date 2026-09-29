use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// The service answered with an error, such as `NoSuchKey`.
    #[error("{code} ({status}): {message}")]
    Service {
        status: u16,
        code: String,
        message: String,
    },
    #[error("request failed: {0}")]
    Transport(String),
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

#[derive(Deserialize)]
struct Envelope {
    error: Body,
}

#[derive(Deserialize)]
struct Body {
    code: String,
    message: String,
}

/// The error an error response carries: `{"error": {"code", "message"}}`, or
/// the status when the body is not that envelope.
pub(crate) fn from_response(status: u16, body: &[u8]) -> StorageError {
    match serde_json::from_slice::<Envelope>(body) {
        Ok(envelope) => StorageError::Service {
            status,
            code: envelope.error.code,
            message: envelope.error.message,
        },
        Err(_) => StorageError::Service {
            status,
            code: "UnknownError".into(),
            message: String::from_utf8_lossy(body).into_owned(),
        },
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

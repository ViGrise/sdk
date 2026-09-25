use http::StatusCode;

/// Errors surfaced to the caller of a VGR4-protected endpoint. `code()` is the
/// stable wire identifier shared with the platform authorize API.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuthError {
  #[error("Request is missing an Authorization header")]
  MissingAuthenticationToken,
  #[error("{0}")]
  IncompleteSignature(String),
  #[error("{0}")]
  InvalidClientTokenId(String),
  #[error("{message}")]
  SignatureDoesNotMatch {
    message: String,
    string_to_sign: Option<String>,
  },
  #[error("{0}")]
  RequestExpired(String),
  #[error("{0}")]
  AccessDenied(String),
  #[error("{0}")]
  ServiceNotAvailableForPlan(String),
  #[error("{message}")]
  ServiceQuotaExceeded {
    message: String,
    quota: Option<crate::QuotaStatus>,
  },
  #[error("{message}")]
  Throttling {
    message: String,
    quota: Option<crate::QuotaStatus>,
  },
  #[error("{message}")]
  LimitExceeded {
    message: String,
    quota: Option<crate::QuotaStatus>,
  },
  #[error("{0}")]
  RequestEntityTooLarge(String),
  #[error("{0}")]
  ServiceUnavailable(String),
  /// Rejection produced by a service's own request resolution, such as an
  /// unknown route or a malformed body.
  #[error("{message}")]
  Rejected {
    status: StatusCode,
    code: String,
    message: String,
  },
}

impl AuthError {
  pub fn code(&self) -> &str {
    match self {
      AuthError::MissingAuthenticationToken => "MissingAuthenticationToken",
      AuthError::IncompleteSignature(_) => "IncompleteSignature",
      AuthError::InvalidClientTokenId(_) => "InvalidClientTokenId",
      AuthError::SignatureDoesNotMatch { .. } => "SignatureDoesNotMatch",
      AuthError::RequestExpired(_) => "RequestExpired",
      AuthError::AccessDenied(_) => "AccessDenied",
      AuthError::ServiceNotAvailableForPlan(_) => "ServiceNotAvailableForPlan",
      AuthError::ServiceQuotaExceeded { .. } => "ServiceQuotaExceeded",
      AuthError::Throttling { .. } => "ThrottlingException",
      AuthError::LimitExceeded { .. } => "LimitExceeded",
      AuthError::RequestEntityTooLarge(_) => "RequestEntityTooLarge",
      AuthError::ServiceUnavailable(_) => "ServiceUnavailable",
      AuthError::Rejected { code, .. } => code,
    }
  }

  pub fn status(&self) -> StatusCode {
    match self {
      AuthError::MissingAuthenticationToken
      | AuthError::IncompleteSignature(_)
      | AuthError::InvalidClientTokenId(_)
      | AuthError::SignatureDoesNotMatch { .. }
      | AuthError::RequestExpired(_) => StatusCode::UNAUTHORIZED,
      AuthError::AccessDenied(_) | AuthError::ServiceNotAvailableForPlan(_) => {
        StatusCode::FORBIDDEN
      }
      AuthError::ServiceQuotaExceeded { .. }
      | AuthError::Throttling { .. }
      | AuthError::LimitExceeded { .. } => StatusCode::TOO_MANY_REQUESTS,
      AuthError::RequestEntityTooLarge(_) => StatusCode::PAYLOAD_TOO_LARGE,
      AuthError::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
      AuthError::Rejected { status, .. } => *status,
    }
  }

  /// The quota a quota, rate or concurrency denial refers to.
  pub fn quota(&self) -> Option<&crate::QuotaStatus> {
    match self {
      AuthError::ServiceQuotaExceeded { quota, .. }
      | AuthError::Throttling { quota, .. }
      | AuthError::LimitExceeded { quota, .. } => quota.as_ref(),
      _ => None,
    }
  }

  /// Mutable access to the quota slot of a quota, rate or concurrency denial.
  pub fn quota_mut(&mut self) -> Option<&mut Option<crate::QuotaStatus>> {
    match self {
      AuthError::ServiceQuotaExceeded { quota, .. }
      | AuthError::Throttling { quota, .. }
      | AuthError::LimitExceeded { quota, .. } => Some(quota),
      _ => None,
    }
  }

  /// Rebuilds an error from a wire `code` and `message`. Unknown codes map to
  /// `AccessDenied` so an unrecognised denial never turns into an allow.
  pub fn from_code(code: &str, message: String) -> Self {
    match code {
      "MissingAuthenticationToken" => AuthError::MissingAuthenticationToken,
      "IncompleteSignature" => AuthError::IncompleteSignature(message),
      "InvalidClientTokenId" => AuthError::InvalidClientTokenId(message),
      "SignatureDoesNotMatch" => AuthError::SignatureDoesNotMatch {
        message,
        string_to_sign: None,
      },
      "RequestExpired" => AuthError::RequestExpired(message),
      "ServiceNotAvailableForPlan" => AuthError::ServiceNotAvailableForPlan(message),
      "ServiceQuotaExceeded" => AuthError::ServiceQuotaExceeded {
        message,
        quota: None,
      },
      "ThrottlingException" => AuthError::Throttling {
        message,
        quota: None,
      },
      "LimitExceeded" => AuthError::LimitExceeded {
        message,
        quota: None,
      },
      _ => AuthError::AccessDenied(message),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn codes_round_trip_through_from_code() {
    for code in [
      "IncompleteSignature",
      "InvalidClientTokenId",
      "SignatureDoesNotMatch",
      "RequestExpired",
      "AccessDenied",
      "ServiceNotAvailableForPlan",
      "ServiceQuotaExceeded",
      "ThrottlingException",
      "LimitExceeded",
    ] {
      assert_eq!(AuthError::from_code(code, "m".into()).code(), code);
    }
  }

  #[test]
  fn quota_rate_and_concurrency_denials_are_429() {
    for code in [
      "ServiceQuotaExceeded",
      "ThrottlingException",
      "LimitExceeded",
    ] {
      let err = AuthError::from_code(code, "m".into());
      assert_eq!(err.status(), StatusCode::TOO_MANY_REQUESTS, "{code}");
    }
  }

  #[test]
  fn unknown_code_is_access_denied() {
    let err = AuthError::from_code("SomethingNew", "m".into());
    assert_eq!(err.code(), "AccessDenied");
    assert_eq!(err.status(), StatusCode::FORBIDDEN);
  }
}

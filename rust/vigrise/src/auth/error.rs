use http::StatusCode;

/// Errors surfaced to the caller of a VGR4-protected endpoint. `code()` is the
/// stable wire identifier shared with the platform authorize API.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AuthError {
  /// The request has no `Authorization` header.
  #[error("Request is missing an Authorization header")]
  MissingAuthenticationToken,
  /// The `Authorization` header or the signed headers are malformed.
  #[error("{0}")]
  IncompleteSignature(String),
  /// The access key ID or session token is not recognized.
  #[error("{0}")]
  InvalidClientTokenId(String),
  /// The computed signature differs from the provided one.
  #[error("{message}")]
  SignatureDoesNotMatch {
    /// Human-readable message.
    message: String,
    /// The string to sign the service computed, for debugging.
    string_to_sign: Option<String>,
  },
  /// The request timestamp is outside the accepted clock skew.
  #[error("{0}")]
  RequestExpired(String),
  /// The role session the request was signed with has expired.
  #[error("{0}")]
  ExpiredToken(String),
  /// The policy evaluation denies the action.
  #[error("{0}")]
  AccessDenied(String),
  /// The service is not part of the caller's tier.
  #[error("{0}")]
  ServiceNotAvailableForTier(String),
  /// A quota of the service is exhausted.
  #[error("{message}")]
  ServiceQuotaExceeded {
    /// Human-readable message.
    message: String,
    /// The quota that refused the request.
    quota: Option<crate::auth::QuotaStatus>,
  },
  /// A spending cap of the organization or the project is reached.
  #[error("{message}")]
  SpendingCapReached {
    /// Human-readable message.
    message: String,
    /// The cap that was reached.
    quota: Option<crate::auth::QuotaStatus>,
  },
  /// Requests arrive faster than a rate quota allows.
  #[error("{message}")]
  Throttling {
    /// Human-readable message.
    message: String,
    /// The quota that throttled the request.
    quota: Option<crate::auth::QuotaStatus>,
  },
  /// A resource limit is reached.
  #[error("{message}")]
  LimitExceeded {
    /// Human-readable message.
    message: String,
    /// The limit that was reached.
    quota: Option<crate::auth::QuotaStatus>,
  },
  /// The request body exceeds the size the service accepts.
  #[error("{0}")]
  RequestEntityTooLarge(String),
  /// The service cannot authorize requests right now.
  #[error("{0}")]
  ServiceUnavailable(String),
  /// Rejection produced by a service's own request resolution, such as an
  /// unknown route or a malformed body.
  #[error("{message}")]
  Rejected {
    /// HTTP status of the rejection.
    status: StatusCode,
    /// Wire error code.
    code: String,
    /// Human-readable message.
    message: String,
  },
}

impl AuthError {
  /// The stable wire identifier of the error.
  pub fn code(&self) -> &str {
    match self {
      AuthError::MissingAuthenticationToken => "MissingAuthenticationToken",
      AuthError::IncompleteSignature(_) => "IncompleteSignature",
      AuthError::InvalidClientTokenId(_) => "InvalidClientTokenId",
      AuthError::SignatureDoesNotMatch { .. } => "SignatureDoesNotMatch",
      AuthError::RequestExpired(_) => "RequestExpired",
      AuthError::ExpiredToken(_) => "ExpiredToken",
      AuthError::AccessDenied(_) => "AccessDenied",
      AuthError::ServiceNotAvailableForTier(_) => "ServiceNotAvailableForTier",
      AuthError::ServiceQuotaExceeded { .. } => "ServiceQuotaExceeded",
      AuthError::SpendingCapReached { .. } => "SpendingCapReached",
      AuthError::Throttling { .. } => "ThrottlingException",
      AuthError::LimitExceeded { .. } => "LimitExceeded",
      AuthError::RequestEntityTooLarge(_) => "RequestEntityTooLarge",
      AuthError::ServiceUnavailable(_) => "ServiceUnavailable",
      AuthError::Rejected { code, .. } => code,
    }
  }

  /// The HTTP status a service answers with.
  pub fn status(&self) -> StatusCode {
    match self {
      AuthError::MissingAuthenticationToken
      | AuthError::IncompleteSignature(_)
      | AuthError::InvalidClientTokenId(_)
      | AuthError::SignatureDoesNotMatch { .. }
      | AuthError::RequestExpired(_)
      | AuthError::ExpiredToken(_) => StatusCode::UNAUTHORIZED,
      AuthError::AccessDenied(_) | AuthError::ServiceNotAvailableForTier(_) => {
        StatusCode::FORBIDDEN
      }
      AuthError::ServiceQuotaExceeded { .. }
      | AuthError::SpendingCapReached { .. }
      | AuthError::Throttling { .. }
      | AuthError::LimitExceeded { .. } => StatusCode::TOO_MANY_REQUESTS,
      AuthError::RequestEntityTooLarge(_) => StatusCode::PAYLOAD_TOO_LARGE,
      AuthError::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
      AuthError::Rejected { status, .. } => *status,
    }
  }

  /// The quota a quota, rate or concurrency denial refers to.
  pub fn quota(&self) -> Option<&crate::auth::QuotaStatus> {
    match self {
      AuthError::ServiceQuotaExceeded { quota, .. }
      | AuthError::SpendingCapReached { quota, .. }
      | AuthError::Throttling { quota, .. }
      | AuthError::LimitExceeded { quota, .. } => quota.as_ref(),
      _ => None,
    }
  }

  /// Mutable access to the quota slot of a quota, rate or concurrency denial.
  pub fn quota_mut(&mut self) -> Option<&mut Option<crate::auth::QuotaStatus>> {
    match self {
      AuthError::ServiceQuotaExceeded { quota, .. }
      | AuthError::SpendingCapReached { quota, .. }
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
      "ExpiredToken" => AuthError::ExpiredToken(message),
      "ServiceNotAvailableForTier" => AuthError::ServiceNotAvailableForTier(message),
      "ServiceQuotaExceeded" => AuthError::ServiceQuotaExceeded {
        message,
        quota: None,
      },
      "SpendingCapReached" => AuthError::SpendingCapReached {
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
      "ServiceNotAvailableForTier",
      "ServiceQuotaExceeded",
      "SpendingCapReached",
      "ThrottlingException",
      "LimitExceeded",
    ] {
      assert_eq!(AuthError::from_code(code, "m".into()).code(), code);
    }
  }

  #[test]
  fn quota_cap_rate_and_concurrency_denials_are_429() {
    for code in [
      "ServiceQuotaExceeded",
      "SpendingCapReached",
      "ThrottlingException",
      "LimitExceeded",
    ] {
      let err = AuthError::from_code(code, "m".into());
      assert_eq!(err.status(), StatusCode::TOO_MANY_REQUESTS, "{code}");
    }
  }

  #[test]
  fn an_expired_session_is_401_expired_token() {
    let err = AuthError::from_code("ExpiredToken", "The security token has expired".into());
    assert_eq!(err.code(), "ExpiredToken");
    assert_eq!(err.status(), StatusCode::UNAUTHORIZED);
  }

  #[test]
  fn unknown_code_is_access_denied() {
    let err = AuthError::from_code("SomethingNew", "m".into());
    assert_eq!(err.code(), "AccessDenied");
    assert_eq!(err.status(), StatusCode::FORBIDDEN);
  }
}

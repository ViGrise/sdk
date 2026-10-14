/// Failures talking to ViGrise Accounts or verifying its tokens.
#[derive(Debug, thiserror::Error)]
pub enum AccountsError {
  /// Accounts could not be reached or answered with a server error.
  #[error("ViGrise Accounts is unavailable: {0}")]
  Unavailable(String),
  /// The client credentials were rejected.
  #[error("ViGrise Accounts rejected the client credentials")]
  InvalidClient,
  /// The client may not introspect SSO tokens.
  #[error("the client is not allowed to introspect SSO tokens")]
  Forbidden,
  /// The introspection rate limit of the client is exhausted.
  #[error("the SSO introspection rate limit is exhausted")]
  RateLimited,
  /// Accounts answered with an unexpected status or body.
  #[error("unexpected answer from ViGrise Accounts: {0}")]
  Protocol(String),
  /// A back-channel `logout_token` failed verification.
  #[error("invalid logout token: {0}")]
  InvalidLogoutToken(String),
  /// An ID token failed verification.
  #[error("invalid ID token: {0}")]
  InvalidIdToken(String),
  /// The authorization code exchange failed.
  #[error("authorization code exchange failed: {0}")]
  TokenExchange(String),
  /// The signing keys could not be loaded or did not contain the key id.
  #[error("signing keys unavailable: {0}")]
  Jwks(String),
}

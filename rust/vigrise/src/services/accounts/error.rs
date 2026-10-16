/// Failures talking to ViGrise Accounts or verifying its tokens.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum AccountsError {
  /// Accounts could not be reached or answered with a server error.
  #[error("ViGrise Accounts is unavailable: {0}")]
  Unavailable(String),
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

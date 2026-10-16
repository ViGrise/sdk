use vigrise::accounts::AccountsError;

/// Failures of the SSO introspection calls.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SsoError {
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
  /// A failure of the shared Accounts helpers.
  #[error(transparent)]
  Accounts(#[from] AccountsError),
}

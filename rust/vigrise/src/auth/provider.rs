use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::auth::credentials::Credentials;
use crate::auth::error::AuthError;

/// The future a [`CredentialsProvider`] returns.
pub type CredentialsFuture<'a> =
  Pin<Box<dyn Future<Output = Result<Credentials, AuthError>> + Send + 'a>>;

/// Supplies the credentials to sign each request with. Long-term keys are a
/// provider of themselves; role sessions come from `sts::AssumeRoleProvider`.
pub trait CredentialsProvider: Send + Sync + fmt::Debug {
  /// Resolves the credentials to sign the next request with.
  fn credentials(&self) -> CredentialsFuture<'_>;

  /// Called when the platform refuses the current credentials, for example a
  /// revoked session. Returns whether the next call can obtain new ones.
  fn invalidate(&self) -> bool {
    false
  }
}

impl CredentialsProvider for Credentials {
  fn credentials(&self) -> CredentialsFuture<'_> {
    let credentials = self.clone();
    Box::pin(async move { Ok(credentials) })
  }
}

impl<T: CredentialsProvider + ?Sized> CredentialsProvider for Arc<T> {
  fn credentials(&self) -> CredentialsFuture<'_> {
    (**self).credentials()
  }

  fn invalidate(&self) -> bool {
    (**self).invalidate()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn long_term_credentials_provide_themselves() {
    let credentials = Credentials::new("VGAKKEY", "secret");
    assert_eq!(credentials.credentials().await.unwrap(), credentials);
  }
}

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use chrono::Utc;
use reqwest::StatusCode;

use crate::credentials::Credentials;
use crate::error::AuthError;
use crate::reqwest_signer::sign_reqwest;
use crate::verify::types::{AuthorizeRequest, AuthorizeResponse};

pub type AuthorizeFuture<'a> =
  Pin<Box<dyn Future<Output = Result<AuthorizeResponse, AuthError>> + Send + 'a>>;

/// Decides whether a request is allowed. `AuthorizeClient` calls the platform;
/// tests substitute their own implementation.
pub trait Authorizer: Send + Sync + 'static {
  fn authorize<'a>(&'a self, request: &'a AuthorizeRequest) -> AuthorizeFuture<'a>;
}

/// Calls the platform authorize endpoint, signing each call with the service's
/// own credentials under the `iam` scope.
pub struct AuthorizeClient {
  http: reqwest::Client,
  endpoint: String,
  credentials: Credentials,
  retry_backoff: Duration,
}

impl AuthorizeClient {
  pub const SIGNING_SERVICE: &'static str = "iam";

  /// `endpoint` is the full URL, for example
  /// `https://platform.vigrise.com/api/internal/v1/authorize`.
  pub fn new(endpoint: impl Into<String>, credentials: Credentials) -> Self {
    let http = reqwest::Client::builder()
      .timeout(Duration::from_secs(2))
      .build()
      .expect("reqwest client builds with static configuration");
    Self {
      http,
      endpoint: endpoint.into(),
      credentials,
      retry_backoff: Duration::from_millis(200),
    }
  }

  async fn attempt(&self, request: &AuthorizeRequest) -> Result<AuthorizeResponse, Attempt> {
    let mut http_request = self
      .http
      .post(&self.endpoint)
      .json(request)
      .build()
      .map_err(|e| Attempt::Fatal(e.to_string()))?;
    sign_reqwest(
      &mut http_request,
      &self.credentials,
      Self::SIGNING_SERVICE,
      Utc::now(),
    )
    .map_err(|e| Attempt::Fatal(e.to_string()))?;

    let response = self
      .http
      .execute(http_request)
      .await
      .map_err(|e| Attempt::Retryable(e.to_string()))?;
    let status = response.status();
    if status.is_server_error() {
      return Err(Attempt::Retryable(format!("platform returned {status}")));
    }
    if status != StatusCode::OK {
      return Err(Attempt::Fatal(format!(
        "platform rejected the service with {status}"
      )));
    }
    response
      .json::<AuthorizeResponse>()
      .await
      .map_err(|e| Attempt::Fatal(format!("invalid authorize response: {e}")))
  }
}

enum Attempt {
  Retryable(String),
  Fatal(String),
}

impl Authorizer for AuthorizeClient {
  fn authorize<'a>(&'a self, request: &'a AuthorizeRequest) -> AuthorizeFuture<'a> {
    Box::pin(async move {
      let first = match self.attempt(request).await {
        Ok(response) => return Ok(response),
        Err(Attempt::Retryable(reason)) => reason,
        Err(Attempt::Fatal(reason)) => {
          tracing::error!(%reason, "authorize call failed");
          return Err(AuthError::ServiceUnavailable(
            "Authorization is unavailable".into(),
          ));
        }
      };
      tracing::warn!(reason = %first, "authorize call failed, retrying once");
      tokio::time::sleep(self.retry_backoff).await;
      self.attempt(request).await.map_err(|attempt| {
        let (Attempt::Retryable(reason) | Attempt::Fatal(reason)) = attempt;
        tracing::error!(%reason, "authorize retry failed");
        AuthError::ServiceUnavailable("Authorization is unavailable".into())
      })
    })
  }
}

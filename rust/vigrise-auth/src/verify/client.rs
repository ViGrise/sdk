use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use reqwest::StatusCode;

use crate::error::AuthError;
use crate::provider::CredentialsProvider;
use crate::reqwest_signer::sign_reqwest;
use crate::verify::types::{AuthorizeRequest, AuthorizeResponse};

/// The future an [`Authorizer`] returns.
pub type AuthorizeFuture<'a> =
  Pin<Box<dyn Future<Output = Result<AuthorizeResponse, AuthError>> + Send + 'a>>;

/// Decides whether a request is allowed. `AuthorizeClient` calls the platform;
/// tests substitute their own implementation.
pub trait Authorizer: Send + Sync + 'static {
  /// Authorizes `request`.
  fn authorize<'a>(&'a self, request: &'a AuthorizeRequest) -> AuthorizeFuture<'a>;

  /// Returns concurrency slots once a request ends. It must not block: the
  /// middleware calls it when a response body finishes or is dropped.
  fn release(&self, leases: Vec<String>) {
    let _ = leases;
  }
}

/// Calls the platform authorize endpoint, signing each call with the service's
/// own credentials under the `iam` scope.
pub struct AuthorizeClient {
  http: reqwest::Client,
  endpoint: String,
  credentials: Arc<dyn CredentialsProvider>,
  retry_backoff: Duration,
}

impl AuthorizeClient {
  /// Scope the calls are signed under.
  pub const SIGNING_SERVICE: &'static str = "iam";

  /// `endpoint` is the full URL, for example
  /// `https://platform.vigrise.com/api/internal/v1/authorize`.
  pub fn new(endpoint: impl Into<String>, credentials: impl CredentialsProvider + 'static) -> Self {
    let http = reqwest::Client::builder()
      .timeout(Duration::from_secs(2))
      .build()
      .expect("reqwest client builds with static configuration");
    Self {
      http,
      endpoint: endpoint.into(),
      credentials: Arc::new(credentials),
      retry_backoff: Duration::from_millis(200),
    }
  }

  async fn attempt(&self, request: &AuthorizeRequest) -> Result<AuthorizeResponse, Attempt> {
    let credentials = self
      .credentials
      .credentials()
      .await
      .map_err(|e| Attempt::Retryable(format!("service credentials are unavailable: {e}")))?;
    let mut http_request = self
      .http
      .post(&self.endpoint)
      .json(request)
      .build()
      .map_err(|e| Attempt::Fatal(e.to_string()))?;
    sign_reqwest(
      &mut http_request,
      &credentials,
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
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
      return Err(Attempt::Refused(format!(
        "platform refused the service's credentials with {status}"
      )));
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
  /// The service's own credentials were refused, as for a revoked or expired
  /// session; a retry uses new ones.
  Refused(String),
  Fatal(String),
}

impl Authorizer for AuthorizeClient {
  fn authorize<'a>(&'a self, request: &'a AuthorizeRequest) -> AuthorizeFuture<'a> {
    Box::pin(async move {
      let first = match self.attempt(request).await {
        Ok(response) => return Ok(response),
        Err(Attempt::Retryable(reason)) => reason,
        Err(Attempt::Refused(reason)) if self.credentials.invalidate() => reason,
        Err(Attempt::Refused(reason) | Attempt::Fatal(reason)) => {
          tracing::error!(%reason, "authorize call failed");
          return Err(AuthError::ServiceUnavailable(
            "Authorization is unavailable".into(),
          ));
        }
      };
      tracing::warn!(reason = %first, "authorize call failed, retrying once");
      tokio::time::sleep(self.retry_backoff).await;
      self.attempt(request).await.map_err(|attempt| {
        let (Attempt::Retryable(reason) | Attempt::Refused(reason) | Attempt::Fatal(reason)) =
          attempt;
        tracing::error!(%reason, "authorize retry failed");
        AuthError::ServiceUnavailable("Authorization is unavailable".into())
      })
    })
  }

  fn release(&self, leases: Vec<String>) {
    if leases.is_empty() {
      return;
    }
    let Some(endpoint) = self
      .endpoint
      .strip_suffix("/authorize")
      .map(|base| format!("{base}/leases/release"))
    else {
      tracing::error!(endpoint = %self.endpoint, "cannot derive the lease release endpoint");
      return;
    };
    let http = self.http.clone();
    let provider = Arc::clone(&self.credentials);
    tokio::spawn(async move {
      let result = async {
        let credentials = provider.credentials().await.map_err(|e| e.to_string())?;
        let mut request = http
          .post(&endpoint)
          .json(&serde_json::json!({ "leases": leases }))
          .build()
          .map_err(|e| e.to_string())?;
        sign_reqwest(
          &mut request,
          &credentials,
          AuthorizeClient::SIGNING_SERVICE,
          Utc::now(),
        )
        .map_err(|e| e.to_string())?;
        let response = http.execute(request).await.map_err(|e| e.to_string())?;
        if response.status().is_success() {
          Ok(())
        } else {
          if matches!(
            response.status(),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
          ) {
            let _ = provider.invalidate();
          }
          Err(format!("platform returned {}", response.status()))
        }
      }
      .await;
      if let Err(reason) = result {
        tracing::warn!(%reason, "releasing concurrency leases failed; they expire on their own");
      }
    });
  }
}

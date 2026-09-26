//! Role sessions from the ViGrise Security Token Service.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::credentials::{Credentials, CredentialsError};
use crate::error::AuthError;
use crate::provider::{CredentialsFuture, CredentialsProvider};
use crate::reqwest_signer::sign_reqwest;

const SIGNING_SERVICE: &str = "sts";
pub const ENV_ROLE_VRN: &str = "VIGRISE_ROLE_VRN";
pub const ENV_STS_URL: &str = "VIGRISE_STS_URL";
/// Sessions at least this long are refreshed `REFRESH_BEFORE_EXPIRY` before
/// they expire; shorter ones after 80 percent of their lifetime.
const LONG_SESSION: chrono::Duration = chrono::Duration::minutes(25);
const REFRESH_BEFORE_EXPIRY: chrono::Duration = chrono::Duration::minutes(5);

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AssumeRoleRequest {
  pub role_vrn: String,
  pub role_session_name: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub duration_seconds: Option<u32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub source_identity: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub session_policy: Option<Value>,
}

impl AssumeRoleRequest {
  pub fn new(role_vrn: impl Into<String>, role_session_name: impl Into<String>) -> Self {
    Self {
      role_vrn: role_vrn.into(),
      role_session_name: role_session_name.into(),
      duration_seconds: None,
      source_identity: None,
      session_policy: None,
    }
  }
}

#[derive(Clone, PartialEq, Deserialize)]
pub struct SessionCredentials {
  pub access_key_id: String,
  pub secret_access_key: String,
  pub session_token: String,
  pub expiration: DateTime<Utc>,
}

impl fmt::Debug for SessionCredentials {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("SessionCredentials")
      .field("access_key_id", &self.access_key_id)
      .field("secret_access_key", &"<redacted>")
      .field("session_token", &"<redacted>")
      .field("expiration", &self.expiration)
      .finish()
  }
}

impl SessionCredentials {
  pub fn to_credentials(&self) -> Credentials {
    Credentials::new(&self.access_key_id, &self.secret_access_key)
      .with_session_token(&self.session_token)
  }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AssumedRole {
  pub role_vrn: String,
  pub session_vrn: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AssumeRoleResponse {
  pub credentials: SessionCredentials,
  pub assumed_role: AssumedRole,
}

#[derive(Deserialize)]
struct ErrorEnvelope {
  error: ErrorDetail,
}

#[derive(Deserialize)]
struct ErrorDetail {
  code: String,
  message: String,
}

fn sts_error(status: reqwest::StatusCode, body: &str) -> AuthError {
  let Ok(ErrorEnvelope { error }) = serde_json::from_str::<ErrorEnvelope>(body) else {
    return AuthError::ServiceUnavailable(format!("STS returned {status}"));
  };
  match error.code.as_str() {
    "AccessDenied" => AuthError::AccessDenied(error.message),
    "InvalidClientTokenId" => AuthError::InvalidClientTokenId(error.message),
    "RequestExpired" => AuthError::RequestExpired(error.message),
    "SignatureDoesNotMatch" => AuthError::SignatureDoesNotMatch {
      message: error.message,
      string_to_sign: None,
    },
    _ if status.is_server_error() => AuthError::ServiceUnavailable(error.message),
    _ => AuthError::Rejected {
      status: http::StatusCode::from_u16(status.as_u16()).unwrap_or(http::StatusCode::BAD_REQUEST),
      code: error.code,
      message: error.message,
    },
  }
}

/// Calls `AssumeRole`, signing with long-term credentials.
#[derive(Clone)]
pub struct StsClient {
  http: reqwest::Client,
  endpoint: String,
  credentials: Arc<dyn CredentialsProvider>,
}

impl fmt::Debug for StsClient {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("StsClient")
      .field("endpoint", &self.endpoint)
      .field("credentials", &self.credentials)
      .finish()
  }
}

impl StsClient {
  /// `endpoint` is the full URL, for example
  /// `https://platform.vigrise.com/api/v1/sts/assume-role`.
  pub fn new(endpoint: impl Into<String>, credentials: impl CredentialsProvider + 'static) -> Self {
    let http = reqwest::Client::builder()
      .timeout(Duration::from_secs(5))
      .build()
      .expect("reqwest client builds with static configuration");
    Self {
      http,
      endpoint: endpoint.into(),
      credentials: Arc::new(credentials),
    }
  }

  pub async fn assume_role(
    &self,
    request: &AssumeRoleRequest,
  ) -> Result<AssumeRoleResponse, AuthError> {
    let credentials = self.credentials.credentials().await?;
    let mut http_request = self
      .http
      .post(&self.endpoint)
      .json(request)
      .build()
      .map_err(|e| AuthError::ServiceUnavailable(e.to_string()))?;
    sign_reqwest(&mut http_request, &credentials, SIGNING_SERVICE, Utc::now())?;
    let response = self
      .http
      .execute(http_request)
      .await
      .map_err(|e| AuthError::ServiceUnavailable(format!("STS is unreachable: {e}")))?;
    let status = response.status();
    let body = response
      .text()
      .await
      .map_err(|e| AuthError::ServiceUnavailable(e.to_string()))?;
    if !status.is_success() {
      return Err(sts_error(status, &body));
    }
    serde_json::from_str(&body)
      .map_err(|e| AuthError::ServiceUnavailable(format!("invalid AssumeRole response: {e}")))
  }
}

struct Cached {
  credentials: Credentials,
  refresh_at: DateTime<Utc>,
}

fn refresh_at(obtained_at: DateTime<Utc>, expiration: DateTime<Utc>) -> DateTime<Utc> {
  let lifetime = expiration - obtained_at;
  if lifetime >= LONG_SESSION {
    expiration - REFRESH_BEFORE_EXPIRY
  } else {
    obtained_at + lifetime * 4 / 5
  }
}

/// Holds one role session and replaces it before it expires. Concurrent
/// callers wait for a single refresh.
pub struct AssumeRoleProvider {
  client: StsClient,
  request: AssumeRoleRequest,
  cached: tokio::sync::Mutex<Option<Cached>>,
}

impl fmt::Debug for AssumeRoleProvider {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("AssumeRoleProvider")
      .field("client", &self.client)
      .field("role_vrn", &self.request.role_vrn)
      .field("role_session_name", &self.request.role_session_name)
      .finish()
  }
}

impl AssumeRoleProvider {
  pub fn new(
    client: StsClient,
    role_vrn: impl Into<String>,
    role_session_name: impl Into<String>,
  ) -> Self {
    Self::with_request(client, AssumeRoleRequest::new(role_vrn, role_session_name))
  }

  pub fn with_request(client: StsClient, request: AssumeRoleRequest) -> Self {
    Self {
      client,
      request,
      cached: tokio::sync::Mutex::new(None),
    }
  }
}

impl CredentialsProvider for AssumeRoleProvider {
  fn credentials(&self) -> CredentialsFuture<'_> {
    Box::pin(async move {
      let mut cached = self.cached.lock().await;
      if let Some(current) = cached.as_ref() {
        if Utc::now() < current.refresh_at {
          return Ok(current.credentials.clone());
        }
      }
      let obtained_at = Utc::now();
      let assumed = self.client.assume_role(&self.request).await?;
      let credentials = assumed.credentials.to_credentials();
      *cached = Some(Cached {
        credentials: credentials.clone(),
        refresh_at: refresh_at(obtained_at, assumed.credentials.expiration),
      });
      Ok(credentials)
    })
  }
}

/// The credentials a ViGrise service signs with: a session of `VIGRISE_ROLE_VRN`
/// obtained through `VIGRISE_STS_URL` with `bootstrap`, or `bootstrap` itself
/// when no role is configured.
pub fn service_credentials(
  bootstrap: Credentials,
  session_name: &str,
) -> Result<Arc<dyn CredentialsProvider>, CredentialsError> {
  service_credentials_with(bootstrap, session_name, |name| std::env::var(name).ok())
}

pub fn service_credentials_with(
  bootstrap: Credentials,
  session_name: &str,
  lookup: impl Fn(&str) -> Option<String>,
) -> Result<Arc<dyn CredentialsProvider>, CredentialsError> {
  let present = |name| lookup(name).filter(|value| !value.is_empty());
  let Some(role_vrn) = present(ENV_ROLE_VRN) else {
    return Ok(Arc::new(bootstrap));
  };
  let endpoint = present(ENV_STS_URL).ok_or_else(|| {
    CredentialsError::Incomplete(format!(
      "{ENV_ROLE_VRN} is set; {ENV_STS_URL} is required too"
    ))
  })?;
  Ok(Arc::new(AssumeRoleProvider::new(
    StsClient::new(endpoint, bootstrap),
    role_vrn,
    session_name,
  )))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn long_sessions_refresh_five_minutes_before_expiry_and_short_ones_at_eighty_percent() {
    let now = Utc::now();
    let hour = now + chrono::Duration::hours(1);
    assert_eq!(refresh_at(now, hour), hour - chrono::Duration::minutes(5));
    let fifteen = now + chrono::Duration::minutes(15);
    assert_eq!(
      refresh_at(now, fifteen),
      now + chrono::Duration::minutes(12)
    );
  }
}

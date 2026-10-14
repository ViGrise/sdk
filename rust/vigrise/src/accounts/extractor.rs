use std::sync::Arc;

use axum::{
  extract::{FromRef, FromRequestParts},
  http::{request::Parts, StatusCode},
  response::{IntoResponse, Response},
  Json,
};

use crate::accounts::{
  sso_token_hash, AccountsError, SsoAccount, SsoIdentity, SsoIntrospector, SsoSession,
};

/// Header an app's frontend server uses to pass the `vigrise_sso` value to its
/// backend.
pub const SSO_HEADER: &str = "x-vigrise-sso";

/// The signed-in person of a request, resolved from the [`SSO_HEADER`] through
/// the shared [`SsoIntrospector`] (`Arc<SsoIntrospector>: FromRef<State>`).
#[derive(Debug, Clone)]
pub struct AccountsIdentity {
  /// The person.
  pub identity: SsoIdentity,
  /// The Accounts session.
  pub session: SsoSession,
  /// Every account of the browser's group.
  pub accounts: Vec<SsoAccount>,
  /// Hash of the SSO token, to store as `sso_hash`.
  pub token_hash: String,
}

/// Rejection of [`AccountsIdentity`].
#[derive(Debug)]
pub enum AccountsRejection {
  /// No active SSO session: 401 `SsoSessionInactive`.
  Inactive,
  /// Accounts is unreachable: 503 `AccountsUnavailable`.
  Unavailable,
  /// The introspection credentials or permission are wrong: 500.
  Misconfigured,
}

impl IntoResponse for AccountsRejection {
  fn into_response(self) -> Response {
    let (status, code) = match self {
      Self::Inactive => (StatusCode::UNAUTHORIZED, "SsoSessionInactive"),
      Self::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "AccountsUnavailable"),
      Self::Misconfigured => (StatusCode::INTERNAL_SERVER_ERROR, "AccountsMisconfigured"),
    };
    (status, Json(serde_json::json!({ "error": code }))).into_response()
  }
}

impl<S> FromRequestParts<S> for AccountsIdentity
where
  Arc<SsoIntrospector>: FromRef<S>,
  S: Send + Sync,
{
  type Rejection = AccountsRejection;

  async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
    let introspector = Arc::<SsoIntrospector>::from_ref(state);
    let token = parts
      .headers
      .get(SSO_HEADER)
      .and_then(|v| v.to_str().ok())
      .filter(|v| !v.is_empty())
      .ok_or(AccountsRejection::Inactive)?;
    let answer = introspector.introspect(token).await.map_err(|e| match e {
      AccountsError::Unavailable(_) | AccountsError::RateLimited => AccountsRejection::Unavailable,
      AccountsError::InvalidClient | AccountsError::Forbidden => {
        tracing::error!(error = %e, "SSO introspection client is misconfigured");
        AccountsRejection::Misconfigured
      }
      _ => AccountsRejection::Unavailable,
    })?;
    match (answer.active, answer.identity, answer.session) {
      (true, Some(identity), Some(session)) => Ok(Self {
        identity,
        session,
        accounts: answer.accounts,
        token_hash: sso_token_hash(token),
      }),
      _ => Err(AccountsRejection::Inactive),
    }
  }
}

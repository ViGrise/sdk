use std::collections::HashMap;
use std::sync::Mutex;

use jsonwebtoken::{decode, decode_header, Algorithm, Validation};
use serde::Deserialize;
use serde_json::Value;

use crate::accounts::{AccountsError, Jwks};

const EVENT: &str = "http://schemas.openid.net/event/backchannel-logout";
const LEEWAY_SECS: i64 = 30;
const MAX_AGE_SECS: i64 = 300;

/// The claims of a verified `logout_token`.
#[derive(Debug, Clone, Deserialize)]
pub struct LogoutToken {
  /// Client-facing subject of the person.
  #[serde(default)]
  pub sub: Option<String>,
  /// Accounts session id that ended.
  #[serde(default)]
  pub sid: Option<String>,
  /// Unique token id.
  pub jti: String,
  iat: i64,
  exp: i64,
  #[serde(default)]
  events: Value,
  #[serde(default)]
  nonce: Option<Value>,
}

/// The result of verifying a `logout_token`.
#[derive(Debug, Clone)]
pub enum BackchannelOutcome {
  /// First delivery of a valid token: end the session.
  Verified(LogoutToken),
  /// A valid token whose `jti` was already processed: answer 200 and do nothing.
  Replayed(LogoutToken),
}

impl BackchannelOutcome {
  /// The verified claims.
  pub fn into_token(self) -> LogoutToken {
    match self {
      Self::Verified(token) | Self::Replayed(token) => token,
    }
  }
}

/// Verifies back-channel `logout_token`s sent by Accounts: signature against
/// the JWKS, `iss`, `aud`, the logout event, the time window and `jti` replay.
pub struct BackchannelLogout {
  issuer: String,
  client_id: String,
  jwks: Jwks,
  seen: Mutex<HashMap<String, i64>>,
}

impl BackchannelLogout {
  /// Creates a verifier for tokens issued by `issuer` for `client_id`.
  pub fn new(issuer: impl Into<String>, client_id: impl Into<String>, jwks: Jwks) -> Self {
    Self {
      issuer: issuer.into(),
      client_id: client_id.into(),
      jwks,
      seen: Mutex::new(HashMap::new()),
    }
  }

  /// Verifies a `logout_token` and records its `jti`. A repeated `jti` of an
  /// otherwise valid token yields [`BackchannelOutcome::Replayed`].
  pub async fn verify(&self, logout_token: &str) -> Result<BackchannelOutcome, AccountsError> {
    let invalid = |m: &str| AccountsError::InvalidLogoutToken(m.to_string());
    let header = decode_header(logout_token).map_err(|e| invalid(&e.to_string()))?;
    if header.alg != Algorithm::RS256 {
      return Err(invalid("unsupported signing algorithm"));
    }
    let key = self.jwks.key(header.kid.as_deref()).await?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[&self.issuer]);
    validation.set_audience(&[&self.client_id]);
    validation.set_required_spec_claims(&["iss", "aud", "exp", "iat"]);
    validation.leeway = LEEWAY_SECS as u64;
    let token = decode::<LogoutToken>(logout_token, &key, &validation)
      .map_err(|e| invalid(&e.to_string()))?
      .claims;

    if token.events.get(EVENT).is_none() {
      return Err(invalid("missing backchannel-logout event"));
    }
    if token.nonce.is_some() {
      return Err(invalid("nonce is not allowed"));
    }
    if token.sid.is_none() && token.sub.is_none() {
      return Err(invalid("neither sid nor sub"));
    }
    if token.jti.is_empty() {
      return Err(invalid("missing jti"));
    }
    let now = chrono::Utc::now().timestamp();
    if token.iat > now + LEEWAY_SECS || token.iat < now - MAX_AGE_SECS - LEEWAY_SECS {
      return Err(invalid("issued outside the accepted time window"));
    }

    let mut seen = self.seen.lock().unwrap();
    seen.retain(|_, exp| *exp >= now);
    if seen.insert(token.jti.clone(), token.exp).is_some() {
      return Ok(BackchannelOutcome::Replayed(token));
    }
    Ok(BackchannelOutcome::Verified(token))
  }
}

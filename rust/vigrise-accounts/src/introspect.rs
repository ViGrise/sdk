use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::AccountsError;

const MAX_CACHE_ENTRIES: usize = 10_000;

/// Hash of a `vigrise_sso` cookie value: lowercase hex SHA-256. Apps store it
/// as `sso_hash` to notice when the cookie changes.
pub fn sso_token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// The person behind an SSO session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SsoIdentity {
    /// Accounts identity id, the `sub` apps key their users by.
    pub id: String,
    /// Primary email address.
    pub email: String,
    /// Display name.
    #[serde(default)]
    pub name: Option<String>,
    /// Avatar URL.
    #[serde(default)]
    pub picture: Option<String>,
    /// Whether the email address is verified.
    #[serde(default)]
    pub email_verified: bool,
}

/// An Accounts session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SsoSession {
    /// Session id, the `sso_sid` of app sessions and the `sid` of logout tokens.
    pub id: String,
    /// When the person authenticated.
    pub authenticated_at: DateTime<Utc>,
    /// Authenticator assurance level.
    pub aal: String,
    /// When the session ends.
    pub expires_at: DateTime<Utc>,
}

/// An account in the browser's account group, for the account menu.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SsoAccount {
    /// Session id to switch to.
    pub session_id: String,
    /// Identity id.
    pub identity_id: String,
    /// Display name.
    #[serde(default)]
    pub name: Option<String>,
    /// Email address.
    #[serde(default)]
    pub email: Option<String>,
    /// Avatar URL.
    #[serde(default)]
    pub picture: Option<String>,
    /// Whether this is the active account.
    pub active: bool,
    /// Whether the session expired and needs re-authentication.
    #[serde(default)]
    pub expired: bool,
}

/// The answer of the introspection endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Introspection {
    /// False for an unknown, revoked or ended token.
    pub active: bool,
    /// The signed-in person.
    #[serde(default)]
    pub identity: Option<SsoIdentity>,
    /// The Accounts session.
    #[serde(default)]
    pub session: Option<SsoSession>,
    /// Every account of the browser's group.
    #[serde(default)]
    pub accounts: Vec<SsoAccount>,
}

/// Connection settings of an [`SsoIntrospector`].
#[derive(Debug, Clone)]
pub struct SsoIntrospectorConfig {
    /// Base URL of the Accounts identity paths reachable from the app server,
    /// for example `http://accounts-backend:8080/o/auth`.
    pub internal_url: String,
    /// OAuth2 client id of the app, registered with `sso_introspection`.
    pub client_id: String,
    /// OAuth2 client secret of the app.
    pub client_secret: String,
    /// How long an answer is reused. Defaults to 30 seconds.
    pub cache_ttl: Duration,
}

impl SsoIntrospectorConfig {
    /// Settings with the default 30 second cache.
    pub fn new(
        internal_url: impl Into<String>,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
    ) -> Self {
        Self {
            internal_url: internal_url.into(),
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            cache_ttl: Duration::from_secs(30),
        }
    }
}

/// Introspects `vigrise_sso` tokens against Accounts and caches the answers by
/// token hash.
pub struct SsoIntrospector {
    http: reqwest::Client,
    config: SsoIntrospectorConfig,
    cache: Mutex<HashMap<String, (Instant, Introspection)>>,
}

impl SsoIntrospector {
    /// Creates an introspector.
    pub fn new(config: SsoIntrospectorConfig) -> Self {
        Self {
            http: reqwest::Client::new(),
            config,
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Replaces the cache lifetime.
    pub fn with_cache_ttl(mut self, ttl: Duration) -> Self {
        self.config.cache_ttl = ttl;
        self
    }

    /// The configured internal base URL.
    pub fn internal_url(&self) -> &str {
        &self.config.internal_url
    }

    /// Introspects a `vigrise_sso` value. Failures are never cached.
    pub async fn introspect(&self, token: &str) -> Result<Introspection, AccountsError> {
        let key = sso_token_hash(token);
        if let Some(answer) = self.cached(&key) {
            return Ok(answer);
        }
        let url = format!(
            "{}/sessions/sso/introspect",
            self.config.internal_url.trim_end_matches('/')
        );
        let response = self
            .http
            .post(url)
            .basic_auth(&self.config.client_id, Some(&self.config.client_secret))
            .json(&serde_json::json!({ "token": token }))
            .send()
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "ViGrise Accounts introspection unreachable");
                AccountsError::Unavailable(e.to_string())
            })?;
        let status = response.status();
        match status.as_u16() {
            200 => {}
            401 => return Err(AccountsError::InvalidClient),
            403 => return Err(AccountsError::Forbidden),
            429 => return Err(AccountsError::RateLimited),
            s if s >= 500 => {
                tracing::warn!(status = s, "ViGrise Accounts introspection failed");
                return Err(AccountsError::Unavailable(format!("status {s}")));
            }
            s => return Err(AccountsError::Protocol(format!("status {s}"))),
        }
        let answer: Introspection = response
            .json()
            .await
            .map_err(|e| AccountsError::Protocol(e.to_string()))?;
        self.store(key, answer.clone());
        Ok(answer)
    }

    /// Drops every cached answer of an Accounts session.
    pub fn invalidate_session(&self, session_id: &str) {
        self.cache.lock().unwrap().retain(|_, (_, answer)| {
            answer.session.as_ref().map(|s| s.id.as_str()) != Some(session_id)
        });
    }

    /// Drops the cached answer of one token, given its [`sso_token_hash`].
    pub fn invalidate_token(&self, token_hash: &str) {
        self.cache.lock().unwrap().remove(token_hash);
    }

    fn cached(&self, key: &str) -> Option<Introspection> {
        let cache = self.cache.lock().unwrap();
        let (at, answer) = cache.get(key)?;
        (at.elapsed() < self.config.cache_ttl).then(|| answer.clone())
    }

    fn store(&self, key: String, answer: Introspection) {
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= MAX_CACHE_ENTRIES {
            let ttl = self.config.cache_ttl;
            cache.retain(|_, (at, _)| at.elapsed() < ttl);
        }
        cache.insert(key, (Instant::now(), answer));
    }
}

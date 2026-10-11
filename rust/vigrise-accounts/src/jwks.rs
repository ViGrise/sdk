use std::sync::Mutex;
use std::time::{Duration, Instant};

use jsonwebtoken::{jwk::JwkSet, DecodingKey};

use crate::AccountsError;

const REFRESH_MIN_INTERVAL: Duration = Duration::from_secs(30);
const CACHE_TTL: Duration = Duration::from_secs(600);

/// Signing keys of ViGrise Accounts, either fixed or fetched from a JWKS URL
/// and cached.
pub struct Jwks {
    source: Source,
}

enum Source {
    Static(JwkSet),
    Remote {
        url: String,
        http: reqwest::Client,
        cache: Mutex<Option<(Instant, JwkSet)>>,
    },
}

impl Jwks {
    /// Uses a fixed key set.
    pub fn from_set(set: JwkSet) -> Self {
        Self {
            source: Source::Static(set),
        }
    }

    /// Fetches the key set from `url`, caches it for ten minutes and
    /// refetches when a token names an unknown key id.
    pub fn remote(url: impl Into<String>) -> Self {
        Self {
            source: Source::Remote {
                url: url.into(),
                http: reqwest::Client::new(),
                cache: Mutex::new(None),
            },
        }
    }

    pub(crate) async fn key(&self, kid: Option<&str>) -> Result<DecodingKey, AccountsError> {
        match &self.source {
            Source::Static(set) => find(set, kid),
            Source::Remote { url, http, cache } => {
                let cached = {
                    let guard = cache.lock().unwrap();
                    guard.as_ref().map(|(at, set)| (*at, set.clone()))
                };
                if let Some((at, set)) = &cached {
                    if at.elapsed() < CACHE_TTL {
                        if let Ok(key) = find(set, kid) {
                            return Ok(key);
                        }
                    }
                    if at.elapsed() < REFRESH_MIN_INTERVAL {
                        return find(set, kid);
                    }
                }
                let set: JwkSet = http
                    .get(url)
                    .send()
                    .await
                    .and_then(|r| r.error_for_status())
                    .map_err(|e| AccountsError::Jwks(e.to_string()))?
                    .json()
                    .await
                    .map_err(|e| AccountsError::Jwks(e.to_string()))?;
                let key = find(&set, kid);
                *cache.lock().unwrap() = Some((Instant::now(), set));
                key
            }
        }
    }
}

fn find(set: &JwkSet, kid: Option<&str>) -> Result<DecodingKey, AccountsError> {
    let jwk = match kid {
        Some(kid) => set.find(kid),
        None if set.keys.len() == 1 => set.keys.first(),
        None => None,
    }
    .ok_or_else(|| AccountsError::Jwks(format!("no signing key for kid {kid:?}")))?;
    DecodingKey::from_jwk(jwk).map_err(|e| AccountsError::Jwks(e.to_string()))
}

//! OAuth2 authorization code helpers for apps that sign in through the
//! redirect flow: PKCE, `state`, code exchange, ID token verification and the
//! end-session URL.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use jsonwebtoken::{decode, decode_header, Algorithm, Validation};
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::{AccountsError, Jwks};

fn random_urlsafe(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::rng().fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

/// A PKCE verifier and its S256 challenge.
#[derive(Debug, Clone)]
pub struct Pkce {
    /// Secret kept by the app and sent with the code exchange.
    pub verifier: String,
    /// Challenge sent with the authorization request.
    pub challenge: String,
}

impl Pkce {
    /// Generates a 43 character verifier from 32 random bytes.
    pub fn generate() -> Self {
        Self::from_verifier(&random_urlsafe(32))
    }

    /// Derives the challenge of an existing verifier.
    pub fn from_verifier(verifier: &str) -> Self {
        Self {
            verifier: verifier.to_string(),
            challenge: URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())),
        }
    }
}

/// A random `state` (or `nonce`) value.
pub fn random_state() -> String {
    random_urlsafe(32)
}

/// An authorization request for the code flow with PKCE.
#[derive(Debug, Clone)]
pub struct AuthorizationRequest {
    /// The `authorization_endpoint` of the discovery document.
    pub authorization_endpoint: String,
    /// OAuth2 client id.
    pub client_id: String,
    /// Registered redirect URI.
    pub redirect_uri: String,
    /// Space separated scopes.
    pub scope: String,
    /// Anti-CSRF value echoed back on the redirect.
    pub state: String,
    /// S256 challenge of the PKCE verifier.
    pub code_challenge: String,
    /// `prompt` parameter such as `none` or `select_account`.
    pub prompt: Option<String>,
    /// `max_age` in seconds.
    pub max_age: Option<u64>,
    /// `login_hint` parameter.
    pub login_hint: Option<String>,
}

impl AuthorizationRequest {
    /// Builds the URL to send the browser to.
    pub fn url(&self) -> Result<String, AccountsError> {
        let mut url = url::Url::parse(&self.authorization_endpoint)
            .map_err(|e| AccountsError::Protocol(e.to_string()))?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("response_type", "code")
                .append_pair("client_id", &self.client_id)
                .append_pair("redirect_uri", &self.redirect_uri)
                .append_pair("scope", &self.scope)
                .append_pair("state", &self.state)
                .append_pair("code_challenge", &self.code_challenge)
                .append_pair("code_challenge_method", "S256");
            if let Some(prompt) = &self.prompt {
                q.append_pair("prompt", prompt);
            }
            if let Some(max_age) = self.max_age {
                q.append_pair("max_age", &max_age.to_string());
            }
            if let Some(hint) = &self.login_hint {
                q.append_pair("login_hint", hint);
            }
        }
        Ok(url.into())
    }
}

/// Builds the RP-initiated logout URL.
pub fn end_session_url(
    end_session_endpoint: &str,
    client_id: &str,
    post_logout_redirect_uri: &str,
    id_token_hint: Option<&str>,
    state: Option<&str>,
) -> Result<String, AccountsError> {
    let mut url = url::Url::parse(end_session_endpoint)
        .map_err(|e| AccountsError::Protocol(e.to_string()))?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("client_id", client_id)
            .append_pair("post_logout_redirect_uri", post_logout_redirect_uri);
        if let Some(hint) = id_token_hint {
            q.append_pair("id_token_hint", hint);
        }
        if let Some(state) = state {
            q.append_pair("state", state);
        }
    }
    Ok(url.into())
}

/// The token endpoint answer.
#[derive(Debug, Clone, Deserialize)]
pub struct TokenResponse {
    /// Access token.
    pub access_token: String,
    /// ID token, present for the `openid` scope.
    #[serde(default)]
    pub id_token: Option<String>,
    /// Refresh token, when granted.
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// Access token lifetime in seconds.
    #[serde(default)]
    pub expires_in: Option<u64>,
}

/// Exchanges authorization codes at the token endpoint.
pub struct OAuth2Client {
    http: reqwest::Client,
    token_endpoint: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
}

impl OAuth2Client {
    /// Creates a confidential client.
    pub fn new(
        token_endpoint: impl Into<String>,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        redirect_uri: impl Into<String>,
    ) -> Self {
        Self {
            http: reqwest::Client::new(),
            token_endpoint: token_endpoint.into(),
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            redirect_uri: redirect_uri.into(),
        }
    }

    /// Exchanges `code` with the PKCE `verifier` of the authorization request.
    pub async fn exchange_code(
        &self,
        code: &str,
        verifier: &str,
    ) -> Result<TokenResponse, AccountsError> {
        let response = self
            .http
            .post(&self.token_endpoint)
            .header("Accept", "application/json")
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", self.redirect_uri.as_str()),
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("code_verifier", verifier),
            ])
            .send()
            .await
            .map_err(|e| AccountsError::Unavailable(e.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(AccountsError::TokenExchange(format!(
                "status {status}: {body}"
            )));
        }
        response
            .json()
            .await
            .map_err(|e| AccountsError::TokenExchange(e.to_string()))
    }
}

/// Claims of a verified ID token.
#[derive(Debug, Clone, Deserialize)]
pub struct IdTokenClaims {
    /// Client-facing subject.
    pub sub: String,
    /// Accounts session id.
    #[serde(default)]
    pub sid: Option<String>,
    /// When the person authenticated, in seconds since the epoch.
    #[serde(default)]
    pub auth_time: Option<i64>,
    /// Nonce echoed from the authorization request.
    #[serde(default)]
    pub nonce: Option<String>,
}

/// Verifies ID tokens issued to one client.
pub struct IdTokenVerifier {
    issuer: String,
    client_id: String,
    jwks: Jwks,
}

impl IdTokenVerifier {
    /// Creates a verifier for ID tokens of `issuer` addressed to `client_id`.
    pub fn new(issuer: impl Into<String>, client_id: impl Into<String>, jwks: Jwks) -> Self {
        Self {
            issuer: issuer.into(),
            client_id: client_id.into(),
            jwks,
        }
    }

    /// Verifies signature, `iss`, `aud`, expiry and, when given, the nonce.
    pub async fn verify(
        &self,
        id_token: &str,
        expected_nonce: Option<&str>,
    ) -> Result<IdTokenClaims, AccountsError> {
        let invalid = |m: String| AccountsError::InvalidIdToken(m);
        let header = decode_header(id_token).map_err(|e| invalid(e.to_string()))?;
        if header.alg != Algorithm::RS256 {
            return Err(invalid("unsupported signing algorithm".into()));
        }
        let key = self.jwks.key(header.kid.as_deref()).await?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[&self.issuer]);
        validation.set_audience(&[&self.client_id]);
        validation.set_required_spec_claims(&["iss", "aud", "exp", "sub"]);
        let claims = decode::<IdTokenClaims>(id_token, &key, &validation)
            .map_err(|e| invalid(e.to_string()))?
            .claims;
        if let Some(expected) = expected_nonce {
            if claims.nonce.as_deref() != Some(expected) {
                return Err(invalid("nonce mismatch".into()));
            }
        }
        Ok(claims)
    }
}

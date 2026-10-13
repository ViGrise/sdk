//! `vigrise login`: sign in with a ViGrise account at Accounts, then exchange
//! the Accounts ID token for a role session at the platform and write it to a
//! profile. The Accounts refresh token is kept so later logins need no browser.

use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::profile::{access_key_id, remove_session, write_private, write_session, SessionProfile};

const SCOPES: &str = "openid email offline_access";
/// The Accounts issuer, where its OpenID configuration is published.
pub const DEFAULT_ACCOUNTS_URL: &str = "https://accounts.vigrise.com/o/oauth2";
/// How long the browser sign-in may take before the listener gives up.
const BROWSER_TIMEOUT: Duration = Duration::from_secs(300);

/// Why a sign-in failed.
#[derive(Debug, thiserror::Error)]
pub enum LoginError {
  /// ViGrise Accounts refused the sign-in or answered unexpectedly.
  #[error("{0}")]
  Accounts(String),
  /// The platform refused to issue the role session.
  #[error("{code}: {message}")]
  Platform {
    /// Error code of the platform.
    code: String,
    /// Human-readable message.
    message: String,
  },
  /// A request failed before an answer arrived.
  #[error("request to {url} failed: {source}")]
  Http {
    /// The URL that was called.
    url: String,
    /// The underlying HTTP error.
    source: reqwest::Error,
  },
  /// A local file could not be read or written.
  #[error("{0}")]
  Io(#[from] std::io::Error),
  /// No role was chosen from the roles on offer.
  #[error("no role was chosen")]
  NoRole,
  /// The profile holds a long-term access key that a session would replace.
  #[error("profile {profile} holds the long-term access key {access_key_id}; choose another profile with --profile")]
  LongTermKeys {
    /// The profile that holds the key.
    profile: String,
    /// The access key ID of the long-term key.
    access_key_id: String,
  },
}

/// Settings of a sign-in.
#[derive(Debug, Clone)]
pub struct LoginOptions {
  /// Accounts issuer, for example `https://accounts.vigrise.com/o/oauth2`.
  pub accounts_url: String,
  /// Platform base URL, for example `https://platform.vigrise.com`.
  pub platform_url: String,
  /// OAuth client ID of the command line at Accounts.
  pub client_id: String,
  /// Profile of the credentials file that receives the session.
  pub profile: String,
  /// Role to assume; when `None`, the person chooses from the roles on offer.
  pub role_vrn: Option<String>,
  /// Project the session acts from; defaults to the role's project.
  pub project_id: Option<String>,
  /// Session length in seconds, from 900 up to the role's maximum.
  pub duration_seconds: Option<u32>,
  /// Sign in with a code shown in the terminal.
  pub device: bool,
  /// Ignore the cached sign-in and open the Accounts account chooser, so another account can
  /// take over the profile. Needs the browser sign-in.
  pub select_account: bool,
  /// Path of the credentials file.
  pub credentials_file: PathBuf,
  /// Directory of the cached Accounts refresh tokens, one file per profile.
  pub sso_dir: PathBuf,
}

/// A role the signed-in person may assume.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AssumableRole {
  /// Project the role belongs to.
  pub project_id: String,
  /// VRN of the role.
  pub role_vrn: String,
  /// Name of the role.
  pub name: String,
  /// Longest session of the role, in seconds.
  pub max_session_seconds: i64,
}

/// What login needs from the person at the terminal.
pub trait Ui: Send + Sync {
  /// Opens `url` in a browser.
  fn open_browser(&self, url: &str);
  /// Shows the code to enter at `verification_uri`, or open `complete` when
  /// it is given.
  fn show_device_code(&self, user_code: &str, verification_uri: &str, complete: Option<&str>);
  /// Returns the index of the chosen role, or `None` when none is chosen.
  fn choose_role(&self, roles: &[AssumableRole]) -> Option<usize>;
}

#[derive(Deserialize)]
struct Discovery {
  authorization_endpoint: String,
  token_endpoint: String,
  device_authorization_endpoint: Option<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
  id_token: Option<String>,
  refresh_token: Option<String>,
}

#[derive(Deserialize)]
struct OAuthError {
  error: String,
  error_description: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Cache {
  refresh_token: String,
}

#[derive(Deserialize)]
struct PlatformError {
  error: PlatformErrorBody,
}

#[derive(Deserialize)]
struct PlatformErrorBody {
  code: String,
  message: String,
}

#[derive(Deserialize)]
struct AssumedCredentials {
  access_key_id: String,
  secret_access_key: String,
  session_token: String,
  expiration: String,
}

#[derive(Deserialize)]
struct Assumed {
  credentials: AssumedCredentials,
  assumed_role: AssumedRoleBody,
}

#[derive(Deserialize)]
struct AssumedRoleBody {
  role_vrn: String,
}

fn http_error(url: &str) -> impl FnOnce(reqwest::Error) -> LoginError + '_ {
  move |source| LoginError::Http {
    url: url.to_string(),
    source,
  }
}

fn random_token(bytes: usize) -> String {
  let mut buffer = vec![0u8; bytes];
  rand::rng().fill_bytes(&mut buffer);
  URL_SAFE_NO_PAD.encode(buffer)
}

/// The OpenID configuration URL of an issuer (OpenID Connect Discovery 4).
pub fn discovery_url(issuer: &str) -> String {
  format!(
    "{}/.well-known/openid-configuration",
    issuer.trim_end_matches('/')
  )
}

async fn discover(http: &reqwest::Client, accounts_url: &str) -> Result<Discovery, LoginError> {
  let url = discovery_url(accounts_url);
  http
    .get(&url)
    .send()
    .await
    .and_then(|response| response.error_for_status())
    .map_err(http_error(&url))?
    .json()
    .await
    .map_err(http_error(&url))
}

/// Posts a token request; `Err(Ok(error))` is an OAuth error answer.
async fn token_request(
  http: &reqwest::Client,
  endpoint: &str,
  form: &[(&str, &str)],
) -> Result<Result<TokenResponse, OAuthError>, LoginError> {
  let response = http
    .post(endpoint)
    .form(form)
    .send()
    .await
    .map_err(http_error(endpoint))?;
  if response.status().is_success() {
    return Ok(Ok(response.json().await.map_err(http_error(endpoint))?));
  }
  let body = response.text().await.map_err(http_error(endpoint))?;
  Ok(Err(serde_json::from_str(&body).unwrap_or(OAuthError {
    error: "server_error".into(),
    error_description: Some(body),
  })))
}

fn describe(error: OAuthError) -> LoginError {
  LoginError::Accounts(match error.error_description {
    Some(description) => format!("{}: {description}", error.error),
    None => error.error,
  })
}

/// How long one connection to the loopback listener may take to send its request.
const REQUEST_READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Reads one request's target and answers it with a short page; `None` for
/// connections that send no complete request in time.
async fn read_callback(
  mut stream: tokio::net::TcpStream,
) -> Option<std::collections::HashMap<String, String>> {
  let mut buffer = Vec::with_capacity(1024);
  let read = tokio::time::timeout(REQUEST_READ_TIMEOUT, async {
    let mut chunk = [0u8; 1024];
    while !buffer.windows(4).any(|w| w == b"\r\n\r\n") && buffer.len() < 16 * 1024 {
      let n = stream.read(&mut chunk).await.ok()?;
      if n == 0 {
        break;
      }
      buffer.extend_from_slice(&chunk[..n]);
    }
    Some(())
  })
  .await;
  if !matches!(read, Ok(Some(()))) {
    return None;
  }
  let request = String::from_utf8_lossy(&buffer).to_string();
  let target = request.split_whitespace().nth(1).unwrap_or("");
  let url = url::Url::parse(&format!("http://127.0.0.1{target}")).ok()?;
  let query: std::collections::HashMap<String, String> = url.query_pairs().into_owned().collect();
  let (status, page) = if url.path() != "/callback" {
    ("404 Not Found", "")
  } else if query.contains_key("code") {
    (
      "200 OK",
      "Signed in. You can close this tab and return to the terminal.",
    )
  } else {
    (
      "200 OK",
      "Sign-in did not complete. Return to the terminal for details.",
    )
  };
  let response = format!(
    "HTTP/1.1 {status}\r\ncontent-type: text/plain; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{page}",
    page.len()
  );
  let _ = stream.write_all(response.as_bytes()).await;
  (url.path() == "/callback").then_some(query)
}

/// Waits for the browser to come back to the loopback listener with the
/// authorization code. Every connection is read on its own, so an idle one
/// holds nothing up, and callbacks carrying another login's state are ignored.
async fn wait_for_code(
  listener: tokio::net::TcpListener,
  state: &str,
) -> Result<String, LoginError> {
  let (sender, mut callbacks) = tokio::sync::mpsc::channel(8);
  loop {
    tokio::select! {
      accepted = listener.accept() => {
        let (stream, _) = accepted?;
        let sender = sender.clone();
        tokio::spawn(async move {
          if let Some(query) = read_callback(stream).await {
            let _ = sender.send(query).await;
          }
        });
      }
      Some(query) = callbacks.recv() => {
        if query.get("state").map(String::as_str) != Some(state) {
          continue;
        }
        if let Some(code) = query.get("code") {
          return Ok(code.clone());
        }
        let reason = query
          .get("error_description")
          .or(query.get("error"))
          .map(|text| text.chars().filter(|c| !c.is_control()).collect::<String>())
          .unwrap_or_else(|| "no authorization code".into());
        return Err(LoginError::Accounts(format!("sign-in failed: {reason}")));
      }
    }
  }
}

async fn browser_sign_in(
  http: &reqwest::Client,
  discovery: &Discovery,
  client_id: &str,
  select_account: bool,
  ui: &dyn Ui,
) -> Result<TokenResponse, LoginError> {
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
  let redirect_uri = format!(
    "http://127.0.0.1:{}/callback",
    listener.local_addr()?.port()
  );
  let verifier = random_token(48);
  let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
  let state = random_token(24);
  let mut url = url::Url::parse(&discovery.authorization_endpoint)
    .map_err(|e| LoginError::Accounts(format!("invalid authorization endpoint: {e}")))?;
  url
    .query_pairs_mut()
    .append_pair("response_type", "code")
    .append_pair("client_id", client_id)
    .append_pair("redirect_uri", &redirect_uri)
    .append_pair("scope", SCOPES)
    .append_pair("state", &state)
    .append_pair("code_challenge", &challenge)
    .append_pair("code_challenge_method", "S256");
  if select_account {
    url
      .query_pairs_mut()
      .append_pair("prompt", "select_account");
  }
  ui.open_browser(url.as_str());
  let code = tokio::time::timeout(BROWSER_TIMEOUT, wait_for_code(listener, &state))
    .await
    .map_err(|_| LoginError::Accounts("sign-in timed out".into()))??;
  token_request(
    http,
    &discovery.token_endpoint,
    &[
      ("grant_type", "authorization_code"),
      ("code", &code),
      ("redirect_uri", &redirect_uri),
      ("client_id", client_id),
      ("code_verifier", &verifier),
    ],
  )
  .await?
  .map_err(describe)
}

#[derive(Deserialize)]
struct DeviceAuthorization {
  device_code: String,
  user_code: String,
  verification_uri: String,
  verification_uri_complete: Option<String>,
  expires_in: u64,
  interval: Option<u64>,
}

async fn device_sign_in(
  http: &reqwest::Client,
  discovery: &Discovery,
  client_id: &str,
  ui: &dyn Ui,
) -> Result<TokenResponse, LoginError> {
  let endpoint = discovery
    .device_authorization_endpoint
    .as_deref()
    .ok_or_else(|| LoginError::Accounts("Accounts offers no device sign-in".into()))?;
  let device: DeviceAuthorization = http
    .post(endpoint)
    .form(&[("client_id", client_id), ("scope", SCOPES)])
    .send()
    .await
    .and_then(|response| response.error_for_status())
    .map_err(http_error(endpoint))?
    .json()
    .await
    .map_err(http_error(endpoint))?;
  ui.show_device_code(
    &device.user_code,
    &device.verification_uri,
    device.verification_uri_complete.as_deref(),
  );
  let mut interval = Duration::from_secs(device.interval.unwrap_or(5).max(1));
  let deadline = tokio::time::Instant::now() + Duration::from_secs(device.expires_in);
  loop {
    tokio::time::sleep(interval).await;
    if tokio::time::Instant::now() >= deadline {
      return Err(LoginError::Accounts("the device code expired".into()));
    }
    match token_request(
      http,
      &discovery.token_endpoint,
      &[
        ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ("device_code", &device.device_code),
        ("client_id", client_id),
      ],
    )
    .await?
    {
      Ok(tokens) => return Ok(tokens),
      Err(error) if error.error == "authorization_pending" => {}
      // RFC 8628 section 3.5: poll five seconds slower from now on.
      Err(error) if error.error == "slow_down" => interval += Duration::from_secs(5),
      Err(error) => return Err(describe(error)),
    }
  }
}

fn cache_path(sso_dir: &Path, profile: &str) -> PathBuf {
  sso_dir.join(format!("{profile}.json"))
}

fn cached_refresh_token(sso_dir: &Path, profile: &str) -> Option<String> {
  let contents = std::fs::read_to_string(cache_path(sso_dir, profile)).ok()?;
  serde_json::from_str::<Cache>(&contents)
    .ok()
    .map(|cache| cache.refresh_token)
}

async fn platform_call<T: serde::de::DeserializeOwned>(
  http: &reqwest::Client,
  url: &str,
  body: &Value,
) -> Result<T, LoginError> {
  let response = http
    .post(url)
    .json(body)
    .send()
    .await
    .map_err(http_error(url))?;
  if response.status().is_success() {
    return response.json().await.map_err(http_error(url));
  }
  let status = response.status();
  let text = response.text().await.map_err(http_error(url))?;
  Err(match serde_json::from_str::<PlatformError>(&text) {
    Ok(error) => LoginError::Platform {
      code: error.error.code,
      message: error.error.message,
    },
    Err(_) => LoginError::Platform {
      code: status.as_u16().to_string(),
      message: text,
    },
  })
}

/// Signs in (silently when a cached sign-in still works), assumes the role and
/// writes the session to the profile. A profile holding a long-term key is
/// refused, since the session would replace a secret shown only once.
pub async fn login(options: &LoginOptions, ui: &dyn Ui) -> Result<SessionProfile, LoginError> {
  if let Some(existing) = access_key_id(&options.credentials_file, &options.profile)? {
    if !existing.starts_with("VGST") {
      return Err(LoginError::LongTermKeys {
        profile: options.profile.clone(),
        access_key_id: existing,
      });
    }
  }
  let http = reqwest::Client::builder()
    .timeout(Duration::from_secs(30))
    .build()
    .expect("reqwest client builds with static configuration");
  if options.select_account && options.device {
    return Err(LoginError::Accounts(
      "choosing an account needs the browser sign-in, not the device code".into(),
    ));
  }
  let discovery = discover(&http, &options.accounts_url).await?;
  let cached = match options.select_account {
    true => None,
    false => cached_refresh_token(&options.sso_dir, &options.profile),
  };
  let refreshed = match cached {
    Some(refresh_token) => token_request(
      &http,
      &discovery.token_endpoint,
      &[
        ("grant_type", "refresh_token"),
        ("refresh_token", &refresh_token),
        ("client_id", &options.client_id),
      ],
    )
    .await?
    .ok()
    .filter(|tokens| tokens.id_token.is_some()),
    None => None,
  };
  let tokens = match refreshed {
    Some(tokens) => tokens,
    None if options.device => device_sign_in(&http, &discovery, &options.client_id, ui).await?,
    None => {
      browser_sign_in(
        &http,
        &discovery,
        &options.client_id,
        options.select_account,
        ui,
      )
      .await?
    }
  };
  let id_token = tokens
    .id_token
    .ok_or_else(|| LoginError::Accounts("Accounts returned no ID token".into()))?;
  match &tokens.refresh_token {
    Some(refresh_token) => write_private(
      &cache_path(&options.sso_dir, &options.profile),
      &serde_json::to_string(&Cache {
        refresh_token: refresh_token.clone(),
      })
      .expect("cache serializes"),
    )?,
    None => {
      let _ = std::fs::remove_file(cache_path(&options.sso_dir, &options.profile));
    }
  }

  let platform = options.platform_url.trim_end_matches('/');
  let role_vrn = match &options.role_vrn {
    Some(role_vrn) => role_vrn.clone(),
    None => {
      let roles: Vec<AssumableRole> = platform_call(
        &http,
        &format!("{platform}/api/v1/sts/assumable-roles"),
        &json!({ "web_identity_token": id_token }),
      )
      .await?;
      let chosen = ui.choose_role(&roles).ok_or(LoginError::NoRole)?;
      roles
        .get(chosen)
        .ok_or(LoginError::NoRole)?
        .role_vrn
        .clone()
    }
  };
  let mut request = json!({ "web_identity_token": id_token, "role_vrn": role_vrn });
  if let Some(project_id) = &options.project_id {
    request["project_id"] = json!(project_id);
  }
  if let Some(duration) = options.duration_seconds {
    request["duration_seconds"] = json!(duration);
  }
  let assumed: Assumed = platform_call(
    &http,
    &format!("{platform}/api/v1/sts/assume-role-with-web-identity"),
    &request,
  )
  .await?;
  let session = SessionProfile {
    access_key_id: assumed.credentials.access_key_id,
    secret_access_key: assumed.credentials.secret_access_key,
    session_token: assumed.credentials.session_token,
    expiration: assumed.credentials.expiration,
    role_vrn: assumed.assumed_role.role_vrn,
  };
  write_session(&options.credentials_file, &options.profile, &session)?;
  Ok(session)
}

/// Forgets the profile's session and cached sign-in.
pub fn logout(credentials_file: &Path, sso_dir: &Path, profile: &str) -> Result<(), LoginError> {
  remove_session(credentials_file, profile)?;
  match std::fs::remove_file(cache_path(sso_dir, profile)) {
    Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
    _ => Ok(()),
  }
}

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use vigrise_cli::login::{login, logout, AssumableRole, LoginOptions, Ui};
use vigrise_cli::profile::read_session;

const CLIENT: &str = "vigrise-cli";

#[derive(Default)]
struct Accounts {
  /// code -> code_challenge
  codes: HashMap<String, String>,
  /// Refresh tokens that still work, with the ID token their refresh returns.
  refresh: HashMap<String, String>,
  device_polls: usize,
  issued: usize,
}

type Shared = Arc<Mutex<Accounts>>;

fn issue(accounts: &mut Accounts) -> Value {
  accounts.issued += 1;
  let n = accounts.issued;
  let refresh = format!("rt-{n}");
  accounts
    .refresh
    .insert(refresh.clone(), format!("idt-{}", n + 1));
  json!({ "access_token": format!("at-{n}"), "token_type": "bearer", "expires_in": 3600,
          "refresh_token": refresh, "id_token": format!("idt-{n}") })
}

async fn authorize(
  State(accounts): State<Shared>,
  Query(query): Query<HashMap<String, String>>,
) -> Response {
  assert_eq!(query["client_id"], CLIENT);
  assert_eq!(query["response_type"], "code");
  assert_eq!(query["code_challenge_method"], "S256");
  assert!(query["scope"].split(' ').any(|s| s == "offline_access"));
  let redirect = url::Url::parse(&query["redirect_uri"]).unwrap();
  assert_eq!(redirect.host_str(), Some("127.0.0.1"));
  assert_eq!(redirect.path(), "/callback");
  let code = format!("code-{}", query["state"].len());
  accounts
    .lock()
    .unwrap()
    .codes
    .insert(code.clone(), query["code_challenge"].clone());
  let location = format!(
    "{}?code={code}&state={}",
    query["redirect_uri"], query["state"]
  );
  (StatusCode::FOUND, [(header::LOCATION, location)]).into_response()
}

async fn token(
  State(accounts): State<Shared>,
  Form(form): Form<HashMap<String, String>>,
) -> Response {
  assert_eq!(form["client_id"], CLIENT);
  let mut accounts = accounts.lock().unwrap();
  match form["grant_type"].as_str() {
    "authorization_code" => {
      let challenge = accounts.codes.remove(&form["code"]).expect("known code");
      let computed = URL_SAFE_NO_PAD.encode(Sha256::digest(form["code_verifier"].as_bytes()));
      assert_eq!(computed, challenge, "PKCE verifier matches");
      Json(issue(&mut accounts)).into_response()
    }
    "refresh_token" => match accounts.refresh.remove(&form["refresh_token"]) {
      Some(_) => Json(issue(&mut accounts)).into_response(),
      None => (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": "invalid_grant" })),
      )
        .into_response(),
    },
    "urn:ietf:params:oauth:grant-type:device_code" => {
      assert_eq!(form["device_code"], "dev-code");
      accounts.device_polls += 1;
      if accounts.device_polls == 1 {
        (
          StatusCode::BAD_REQUEST,
          Json(json!({ "error": "authorization_pending" })),
        )
          .into_response()
      } else {
        Json(issue(&mut accounts)).into_response()
      }
    }
    other => panic!("unexpected grant {other}"),
  }
}

async fn device(Form(form): Form<HashMap<String, String>>) -> Json<Value> {
  assert_eq!(form["client_id"], CLIENT);
  Json(
    json!({ "device_code": "dev-code", "user_code": "WDJB-MJHT", "verification_uri": "https://accounts.test/oauth2/device",
               "verification_uri_complete": "https://accounts.test/oauth2/device?user_code=WDJB-MJHT", "expires_in": 600, "interval": 1 }),
  )
}

#[derive(Default)]
struct Platform {
  assumed: Vec<Value>,
}

async fn assumable(
  State(platform): State<Arc<Mutex<Platform>>>,
  Json(body): Json<Value>,
) -> Json<Value> {
  assert!(body["web_identity_token"]
    .as_str()
    .unwrap()
    .starts_with("idt-"));
  let _ = platform;
  Json(json!([
    { "project_id": "p", "role_vrn": "vrn:vigrise:iam:p:role/dev", "name": "dev", "max_session_seconds": 3600 },
    { "project_id": "p", "role_vrn": "vrn:vigrise:iam:p:role/ops", "name": "ops", "max_session_seconds": 3600 }
  ]))
}

async fn assume(
  State(platform): State<Arc<Mutex<Platform>>>,
  Json(body): Json<Value>,
) -> Json<Value> {
  let token = body["web_identity_token"].as_str().unwrap().to_string();
  platform.lock().unwrap().assumed.push(body.clone());
  Json(json!({
    "credentials": { "access_key_id": format!("VGST{}", token.to_uppercase().replace('-', "")), "secret_access_key": "secret",
                     "session_token": format!("token-for-{token}"), "expiration": "2026-09-26T10:30:00Z" },
    "assumed_role": { "role_vrn": body["role_vrn"], "session_vrn": "vrn:vigrise:sts:p:assumed-role/dev/alice" }
  }))
}

async fn serve() -> (String, String, Shared, Arc<Mutex<Platform>>) {
  let accounts: Shared = Arc::default();
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let accounts_url = format!("http://{}/o", listener.local_addr().unwrap());
  let discovery = json!({
    "issuer": accounts_url, "authorization_endpoint": format!("{accounts_url}/oauth2/auth"),
    "token_endpoint": format!("{accounts_url}/oauth2/token"),
    "device_authorization_endpoint": format!("{accounts_url}/oauth2/device/auth")
  });
  let app = Router::new()
    .route(
      "/o/.well-known/openid-configuration",
      get(move || {
        let d = discovery.clone();
        async move { Json(d) }
      }),
    )
    .route("/o/oauth2/auth", get(authorize))
    .route("/o/oauth2/token", post(token))
    .route("/o/oauth2/device/auth", post(device))
    .with_state(accounts.clone());
  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  let platform: Arc<Mutex<Platform>> = Arc::default();
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let platform_url = format!("http://{}", listener.local_addr().unwrap());
  let app = Router::new()
    .route("/api/v1/sts/assumable-roles", post(assumable))
    .route("/api/v1/sts/assume-role-with-web-identity", post(assume))
    .with_state(platform.clone());
  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  (accounts_url, platform_url, accounts, platform)
}

/// Plays the person at the terminal: follows the browser URL, remembers the
/// device code it was shown, and picks the last role offered.
#[derive(Default)]
struct FakeUser {
  opened: Mutex<Vec<String>>,
  device_codes: Mutex<Vec<String>>,
  offered: Mutex<Vec<String>>,
}

impl Ui for FakeUser {
  fn open_browser(&self, url: &str) {
    self.opened.lock().unwrap().push(url.to_string());
    let url = url.to_string();
    tokio::spawn(async move {
      let _ = reqwest::get(url).await;
    });
  }

  fn show_device_code(&self, user_code: &str, _verification_uri: &str, _complete: Option<&str>) {
    self
      .device_codes
      .lock()
      .unwrap()
      .push(user_code.to_string());
  }

  fn choose_role(&self, roles: &[AssumableRole]) -> Option<usize> {
    *self.offered.lock().unwrap() = roles.iter().map(|r| r.role_vrn.clone()).collect();
    Some(roles.len() - 1)
  }
}

fn options(dir: &std::path::Path, accounts_url: &str, platform_url: &str) -> LoginOptions {
  LoginOptions {
    accounts_url: accounts_url.into(),
    platform_url: platform_url.into(),
    client_id: CLIENT.into(),
    profile: "dev".into(),
    role_vrn: Some("vrn:vigrise:iam:p:role/dev".into()),
    project_id: None,
    duration_seconds: Some(900),
    device: false,
    credentials_file: dir.join("credentials"),
    sso_dir: dir.join("sso"),
  }
}

#[tokio::test]
async fn a_browser_login_writes_the_profile_and_the_next_login_is_silent() {
  let (accounts_url, platform_url, _accounts, platform) = serve().await;
  let dir = tempfile::tempdir().unwrap();
  let user = FakeUser::default();

  let first = login(&options(dir.path(), &accounts_url, &platform_url), &user)
    .await
    .unwrap();
  assert_eq!(user.opened.lock().unwrap().len(), 1);
  assert_eq!(first.access_key_id, "VGSTIDT1");
  let written = read_session(&dir.path().join("credentials"), "dev")
    .unwrap()
    .unwrap();
  assert_eq!(written.session_token, "token-for-idt-1");
  assert_eq!(written.role_vrn, "vrn:vigrise:iam:p:role/dev");
  assert_eq!(platform.lock().unwrap().assumed[0]["duration_seconds"], 900);
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    let cache = dir.path().join("sso").join("dev.json");
    assert_eq!(
      std::fs::metadata(&cache).unwrap().permissions().mode() & 0o777,
      0o600
    );
  }

  let second = login(&options(dir.path(), &accounts_url, &platform_url), &user)
    .await
    .unwrap();
  assert_eq!(
    user.opened.lock().unwrap().len(),
    1,
    "no browser for a cached sign-in"
  );
  assert_eq!(second.access_key_id, "VGSTIDT2");
}

#[tokio::test]
async fn a_device_login_shows_the_code_and_waits_for_approval() {
  let (accounts_url, platform_url, accounts, _) = serve().await;
  let dir = tempfile::tempdir().unwrap();
  let user = FakeUser::default();
  let mut options = options(dir.path(), &accounts_url, &platform_url);
  options.device = true;
  let session = login(&options, &user).await.unwrap();
  assert_eq!(
    *user.device_codes.lock().unwrap(),
    vec!["WDJB-MJHT".to_string()]
  );
  assert!(user.opened.lock().unwrap().is_empty());
  assert_eq!(accounts.lock().unwrap().device_polls, 2);
  assert!(session.access_key_id.starts_with("VGST"));
}

#[tokio::test]
async fn without_a_role_the_user_picks_one_the_platform_offers() {
  let (accounts_url, platform_url, _, platform) = serve().await;
  let dir = tempfile::tempdir().unwrap();
  let user = FakeUser::default();
  let mut options = options(dir.path(), &accounts_url, &platform_url);
  options.role_vrn = None;
  login(&options, &user).await.unwrap();
  assert_eq!(user.offered.lock().unwrap().len(), 2);
  assert_eq!(
    platform.lock().unwrap().assumed[0]["role_vrn"],
    "vrn:vigrise:iam:p:role/ops"
  );
}

#[tokio::test]
async fn an_expired_sign_in_falls_back_to_the_browser_and_logout_forgets_it() {
  let (accounts_url, platform_url, accounts, _) = serve().await;
  let dir = tempfile::tempdir().unwrap();
  let user = FakeUser::default();
  let options = options(dir.path(), &accounts_url, &platform_url);
  login(&options, &user).await.unwrap();
  accounts.lock().unwrap().refresh.clear();
  login(&options, &user).await.unwrap();
  assert_eq!(user.opened.lock().unwrap().len(), 2);

  logout(&options.credentials_file, &options.sso_dir, "dev").unwrap();
  assert!(read_session(&options.credentials_file, "dev")
    .unwrap()
    .is_none());
  assert!(!dir.path().join("sso").join("dev.json").exists());
}

#[test]
fn the_default_accounts_url_is_the_production_issuer() {
  assert_eq!(
    vigrise_cli::login::discovery_url(vigrise_cli::login::DEFAULT_ACCOUNTS_URL),
    "https://accounts.vigrise.com/o/oauth2/.well-known/openid-configuration"
  );
  assert_eq!(
    vigrise_cli::login::discovery_url("https://accounts.vigrise.com/o/oauth2/"),
    "https://accounts.vigrise.com/o/oauth2/.well-known/openid-configuration"
  );
}

#[tokio::test]
async fn a_profile_holding_long_term_keys_is_left_alone() {
  let (accounts_url, platform_url, _, _) = serve().await;
  let dir = tempfile::tempdir().unwrap();
  let original = "[dev]\nvigrise_access_key_id = VGAKLONGTERM\nvigrise_secret_access_key = s\n";
  std::fs::write(dir.path().join("credentials"), original).unwrap();
  let user = FakeUser::default();
  let error = login(&options(dir.path(), &accounts_url, &platform_url), &user)
    .await
    .unwrap_err();
  assert!(error.to_string().contains("VGAKLONGTERM"), "{error}");
  assert!(
    user.opened.lock().unwrap().is_empty(),
    "refused before signing in"
  );
  assert_eq!(
    std::fs::read_to_string(dir.path().join("credentials")).unwrap(),
    original
  );
}

/// Opens an idle connection and sends a callback with a foreign state to the
/// loopback listener before following the real sign-in.
#[derive(Default)]
struct NoisyBrowser {
  inner: FakeUser,
}

impl Ui for NoisyBrowser {
  fn open_browser(&self, url: &str) {
    let redirect = url::Url::parse(url)
      .unwrap()
      .query_pairs()
      .find(|(key, _)| key == "redirect_uri")
      .map(|(_, value)| value.to_string())
      .unwrap();
    let port = url::Url::parse(&redirect).unwrap().port().unwrap();
    let url = url.to_string();
    tokio::spawn(async move {
      let idle = tokio::net::TcpStream::connect(("127.0.0.1", port))
        .await
        .unwrap();
      let _ = reqwest::get(format!(
        "http://127.0.0.1:{port}/callback?code=stolen&state=someone-else"
      ))
      .await;
      let _ = reqwest::get(url).await;
      drop(idle);
    });
    self.inner.opened.lock().unwrap().push(redirect);
  }

  fn show_device_code(&self, user_code: &str, verification_uri: &str, complete: Option<&str>) {
    self
      .inner
      .show_device_code(user_code, verification_uri, complete);
  }

  fn choose_role(&self, roles: &[AssumableRole]) -> Option<usize> {
    self.inner.choose_role(roles)
  }
}

#[tokio::test]
async fn idle_connections_and_foreign_callbacks_do_not_stop_the_sign_in() {
  let (accounts_url, platform_url, _, _) = serve().await;
  let dir = tempfile::tempdir().unwrap();
  let browser = NoisyBrowser::default();
  let session = tokio::time::timeout(
    std::time::Duration::from_secs(20),
    login(&options(dir.path(), &accounts_url, &platform_url), &browser),
  )
  .await
  .expect("sign-in completes")
  .unwrap();
  assert_eq!(session.access_key_id, "VGSTIDT1");
}

#![cfg(feature = "sts")]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use chrono::Utc;
use serde_json::{json, Value};
use vigrise_auth::sts::{AssumeRoleProvider, StsClient};
use vigrise_auth::{AuthError, AuthorizationHeader, Credentials, CredentialsProvider};

#[derive(Clone)]
struct Sts {
  calls: Arc<AtomicUsize>,
  /// Lifetime of the sessions the server issues.
  lifetime: Duration,
  delay: Duration,
  status: StatusCode,
  seen: Arc<Mutex<Vec<(String, Option<String>)>>>,
}

async fn assume_role(
  State(sts): State<Sts>,
  headers: HeaderMap,
  Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
  let n = sts.calls.fetch_add(1, Ordering::SeqCst);
  let auth = AuthorizationHeader::parse(headers["authorization"].to_str().unwrap()).unwrap();
  assert_eq!(auth.service, "sts");
  assert_eq!(auth.access_key_id, "VGAKBOOTSTRAP");
  assert_eq!(body["role_vrn"], "vrn:vigrise:iam:p:role/tts-service");
  assert_eq!(body["role_session_name"], "tts-gateway");
  tokio::time::sleep(sts.delay).await;
  if sts.status != StatusCode::OK {
    return (
      sts.status,
      Json(json!({ "error": { "code": "AccessDenied", "message": "not trusted" } })),
    );
  }
  let expiration = Utc::now() + chrono::Duration::from_std(sts.lifetime).unwrap();
  (
    StatusCode::OK,
    Json(json!({
      "credentials": {
        "access_key_id": format!("VGSTSESSION{n:09}"),
        "secret_access_key": "session-secret",
        "session_token": format!("token-{n}"),
        "expiration": expiration.to_rfc3339(),
      },
      "assumed_role": {
        "role_vrn": "vrn:vigrise:iam:p:role/tts-service",
        "session_vrn": "vrn:vigrise:sts:p:assumed-role/tts-service/tts-gateway"
      }
    })),
  )
}

async fn record_signer(State(sts): State<Sts>, headers: HeaderMap) -> (StatusCode, Json<Value>) {
  let auth = AuthorizationHeader::parse(headers["authorization"].to_str().unwrap()).unwrap();
  let token = headers
    .get("x-vgr-security-token")
    .map(|v| v.to_str().unwrap().to_string());
  sts.seen.lock().unwrap().push((auth.access_key_id, token));
  (
    StatusCode::OK,
    Json(json!({
      "decision": "Allow",
      "principal": {
        "vrn": "vrn:vigrise:iam:p:role/tts-service", "id": "00000000-0000-0000-0000-000000000000",
        "type": "role", "name": "tts-service",
        "project_id": "00000000-0000-0000-0000-000000000000",
        "organization_id": "00000000-0000-0000-0000-000000000000"
      },
      "project_plan": "Pro"
    })),
  )
}

async fn serve(lifetime: Duration, delay: Duration, status: StatusCode) -> (String, Sts) {
  let sts = Sts {
    calls: Arc::new(AtomicUsize::new(0)),
    lifetime,
    delay,
    status,
    seen: Arc::default(),
  };
  let app = Router::new()
    .route("/api/v1/sts/assume-role", post(assume_role))
    .route("/api/internal/v1/authorize", post(record_signer))
    .with_state(sts.clone());
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let addr = listener.local_addr().unwrap();
  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  (format!("http://{addr}"), sts)
}

fn provider(base: &str) -> AssumeRoleProvider {
  let client = StsClient::new(
    format!("{base}/api/v1/sts/assume-role"),
    Credentials::new("VGAKBOOTSTRAP", "bootstrap-secret"),
  );
  AssumeRoleProvider::new(client, "vrn:vigrise:iam:p:role/tts-service", "tts-gateway")
}

#[tokio::test]
async fn a_session_is_reused_until_its_refresh_window() {
  let (base, sts) = serve(Duration::from_secs(3600), Duration::ZERO, StatusCode::OK).await;
  let provider = provider(&base);
  let first = provider.credentials().await.unwrap();
  let second = provider.credentials().await.unwrap();
  assert_eq!(first, second);
  assert!(first.access_key_id.starts_with("VGST"));
  assert_eq!(first.session_token.as_deref(), Some("token-0"));
  assert_eq!(sts.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_short_session_is_refreshed_after_most_of_its_lifetime() {
  let (base, sts) = serve(Duration::from_secs(1), Duration::ZERO, StatusCode::OK).await;
  let provider = provider(&base);
  let first = provider.credentials().await.unwrap();
  tokio::time::sleep(Duration::from_millis(900)).await;
  let second = provider.credentials().await.unwrap();
  assert_ne!(first.access_key_id, second.access_key_id);
  assert_eq!(sts.calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn concurrent_callers_share_one_refresh() {
  let (base, sts) = serve(
    Duration::from_secs(3600),
    Duration::from_millis(100),
    StatusCode::OK,
  )
  .await;
  let provider = Arc::new(provider(&base));
  let calls: Vec<_> = (0..10)
    .map(|_| {
      let provider = provider.clone();
      tokio::spawn(async move { provider.credentials().await.unwrap() })
    })
    .collect();
  for call in calls {
    call.await.unwrap();
  }
  assert_eq!(sts.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_refused_assumption_is_an_error() {
  let (base, _) = serve(
    Duration::from_secs(3600),
    Duration::ZERO,
    StatusCode::FORBIDDEN,
  )
  .await;
  let error = provider(&base).credentials().await.unwrap_err();
  assert!(matches!(error, AuthError::AccessDenied(_)), "{error:?}");
}

#[cfg(feature = "verify")]
#[tokio::test]
async fn the_authorize_client_signs_with_the_provider_session() {
  use vigrise_auth::verify::{AuthorizeClient, AuthorizeRequest, Authorizer};
  let (base, sts) = serve(Duration::from_secs(3600), Duration::ZERO, StatusCode::OK).await;
  let client = AuthorizeClient::new(
    format!("{base}/api/internal/v1/authorize"),
    Arc::new(provider(&base)),
  );
  let request = AuthorizeRequest {
    security_token: None,
    access_key_id: "VGAKUSER".into(),
    string_to_sign: "sts".into(),
    signature: "sig".into(),
    action: "tts:SynthesizeSpeech".into(),
    resources: vec![],
    context: Default::default(),
    usage: Default::default(),
  };
  client.authorize(&request).await.unwrap();
  let seen = sts.seen.lock().unwrap().clone();
  assert_eq!(
    seen,
    vec![(
      "VGSTSESSION000000000".to_string(),
      Some("token-0".to_string())
    )]
  );
}

#[tokio::test]
async fn service_credentials_use_the_role_session_when_a_role_is_configured() {
  use vigrise_auth::sts::service_credentials_with;
  let (base, sts) = serve(Duration::from_secs(3600), Duration::ZERO, StatusCode::OK).await;
  let bootstrap = Credentials::new("VGAKBOOTSTRAP", "bootstrap-secret");
  let env = |role: Option<&'static str>, url: Option<String>| {
    move |name: &str| match name {
      "VIGRISE_ROLE_VRN" => role.map(str::to_string),
      "VIGRISE_STS_URL" => url.clone(),
      _ => None,
    }
  };

  let long_term =
    service_credentials_with(bootstrap.clone(), "tts-gateway", env(None, None)).unwrap();
  assert_eq!(long_term.credentials().await.unwrap(), bootstrap);

  let missing_url = service_credentials_with(
    bootstrap.clone(),
    "tts-gateway",
    env(Some("vrn:vigrise:iam:p:role/tts-service"), None),
  );
  assert!(missing_url
    .unwrap_err()
    .to_string()
    .contains("VIGRISE_STS_URL"));

  let session = service_credentials_with(
    bootstrap,
    "tts-gateway",
    env(
      Some("vrn:vigrise:iam:p:role/tts-service"),
      Some(format!("{base}/api/v1/sts/assume-role")),
    ),
  )
  .unwrap();
  assert!(session
    .credentials()
    .await
    .unwrap()
    .access_key_id
    .starts_with("VGST"));
  assert_eq!(sts.calls.load(Ordering::SeqCst), 1);
}

#![cfg(feature = "sts")]

use std::sync::atomic::{AtomicU16, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use chrono::Utc;
use serde_json::{Value, json};
use vigrise::auth::sts::{AssumeRoleProvider, StsClient};
use vigrise::auth::{AuthError, AuthorizationHeader, Credentials, CredentialsProvider};

type SeenSessions = Arc<Mutex<Vec<(String, Option<String>)>>>;

#[derive(Clone)]
struct Sts {
  calls: Arc<AtomicUsize>,
  /// Lifetime of the sessions the server issues.
  lifetime: Duration,
  delay: Duration,
  status: Arc<AtomicU16>,
  seen: SeenSessions,
  /// Sessions the fake platform refuses, as after a revocation.
  rejected: Arc<Mutex<Vec<String>>>,
}

impl Sts {
  fn fail_with(&self, status: StatusCode) {
    self.status.store(status.as_u16(), Ordering::SeqCst);
  }
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
  assert_eq!(body["role_vrn"], "vrn:vigrise:iam:p:role/speech-service");
  assert_eq!(body["role_session_name"], "speech-gateway");
  tokio::time::sleep(sts.delay).await;
  let status = StatusCode::from_u16(sts.status.load(Ordering::SeqCst)).unwrap();
  if status != StatusCode::OK {
    return (
      status,
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
        "role_vrn": "vrn:vigrise:iam:p:role/speech-service",
        "session_vrn": "vrn:vigrise:sts:p:assumed-role/speech-service/speech-gateway"
      }
    })),
  )
}

async fn record_signer(State(sts): State<Sts>, headers: HeaderMap) -> (StatusCode, Json<Value>) {
  let auth = AuthorizationHeader::parse(headers["authorization"].to_str().unwrap()).unwrap();
  let token = headers
    .get("x-vgr-security-token")
    .map(|v| v.to_str().unwrap().to_string());
  let refused = sts.rejected.lock().unwrap().contains(&auth.access_key_id);
  sts.seen.lock().unwrap().push((auth.access_key_id, token));
  if refused {
    return (
      StatusCode::FORBIDDEN,
      Json(
        json!({ "error": { "code": "AccessDenied", "message": "The role session has been revoked" } }),
      ),
    );
  }
  (
    StatusCode::OK,
    Json(json!({
      "decision": "Allow",
      "principal": {
        "vrn": "vrn:vigrise:iam:p:role/speech-service", "id": "00000000-0000-0000-0000-000000000000",
        "type": "role", "name": "speech-service",
        "project_id": "00000000-0000-0000-0000-000000000000",
        "organization_id": "00000000-0000-0000-0000-000000000000"
      },
      "tier": "Pro"
    })),
  )
}

async fn serve(lifetime: Duration, delay: Duration, status: StatusCode) -> (String, Sts) {
  let sts = Sts {
    calls: Arc::new(AtomicUsize::new(0)),
    lifetime,
    delay,
    status: Arc::new(AtomicU16::new(status.as_u16())),
    seen: Arc::default(),
    rejected: Arc::default(),
  };
  let app = Router::new()
    .route("/v1/assume-role", post(assume_role))
    .route("/api/internal/v1/authorize", post(record_signer))
    .with_state(sts.clone());
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let addr = listener.local_addr().unwrap();
  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  (format!("http://{addr}"), sts)
}

fn provider(base: &str) -> AssumeRoleProvider {
  let client = StsClient::new(
    format!("{base}/v1/assume-role"),
    Credentials::new("VGAKBOOTSTRAP", "bootstrap-secret"),
  );
  AssumeRoleProvider::new(
    client,
    "vrn:vigrise:iam:p:role/speech-service",
    "speech-gateway",
  )
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
  use vigrise::auth::verify::{AuthorizeClient, AuthorizeRequest, Authorizer};
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
    action: "speech:SynthesizeSpeech".into(),
    resources: vec![],
    context: Default::default(),
    usage: Default::default(),
    streaming_signing_key: false,
    presigned_expires: None,
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
  use vigrise::auth::sts::service_credentials_with;
  let (base, sts) = serve(Duration::from_secs(3600), Duration::ZERO, StatusCode::OK).await;
  let bootstrap = Credentials::new("VGAKBOOTSTRAP", "bootstrap-secret");
  let env = |role: Option<&'static str>, url: Option<String>| {
    move |name: &str| match name {
      "VIGRISE_ROLE_VRN" => role.map(str::to_string),
      "VIGRISE_ENDPOINT_URL_STS" => url.clone(),
      _ => None,
    }
  };

  let long_term =
    service_credentials_with(bootstrap.clone(), "speech-gateway", env(None, None)).unwrap();
  assert_eq!(long_term.credentials().await.unwrap(), bootstrap);

  let session = service_credentials_with(
    bootstrap,
    "speech-gateway",
    env(
      Some("vrn:vigrise:iam:p:role/speech-service"),
      Some(format!("{base}/v1/assume-role")),
    ),
  )
  .unwrap();
  assert!(
    session
      .credentials()
      .await
      .unwrap()
      .access_key_id
      .starts_with("VGST")
  );
  assert_eq!(sts.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_valid_session_outlives_a_failed_refresh_and_failures_back_off() {
  let (base, sts) = serve(Duration::from_secs(1), Duration::ZERO, StatusCode::OK).await;
  let provider = provider(&base);
  let first = provider.credentials().await.unwrap();
  sts.fail_with(StatusCode::SERVICE_UNAVAILABLE);
  tokio::time::sleep(Duration::from_millis(850)).await;
  assert_eq!(provider.credentials().await.unwrap(), first);
  assert_eq!(provider.credentials().await.unwrap(), first);
  assert_eq!(sts.calls.load(Ordering::SeqCst), 2);
  tokio::time::sleep(Duration::from_millis(300)).await;
  assert!(provider.credentials().await.is_err());
}

#[tokio::test]
async fn an_invalidated_session_is_replaced() {
  let (base, sts) = serve(Duration::from_secs(3600), Duration::ZERO, StatusCode::OK).await;
  let provider = provider(&base);
  let first = provider.credentials().await.unwrap();
  assert!(provider.invalidate());
  let second = provider.credentials().await.unwrap();
  assert_ne!(first.access_key_id, second.access_key_id);
  assert_eq!(sts.calls.load(Ordering::SeqCst), 2);
}

#[cfg(feature = "verify")]
#[tokio::test]
async fn the_authorize_client_replaces_a_rejected_session_and_retries() {
  use vigrise::auth::verify::{AuthorizeClient, AuthorizeRequest, Authorizer};
  let (base, sts) = serve(Duration::from_secs(3600), Duration::ZERO, StatusCode::OK).await;
  sts
    .rejected
    .lock()
    .unwrap()
    .push("VGSTSESSION000000000".into());
  let client = AuthorizeClient::new(
    format!("{base}/api/internal/v1/authorize"),
    Arc::new(provider(&base)),
  );
  let request = AuthorizeRequest {
    security_token: None,
    access_key_id: "VGAKUSER".into(),
    string_to_sign: "sts".into(),
    signature: "sig".into(),
    action: "speech:SynthesizeSpeech".into(),
    resources: vec![],
    context: Default::default(),
    usage: Default::default(),
    streaming_signing_key: false,
    presigned_expires: None,
  };
  client.authorize(&request).await.unwrap();
  let signers: Vec<String> = sts
    .seen
    .lock()
    .unwrap()
    .iter()
    .map(|(key, _)| key.clone())
    .collect();
  assert_eq!(
    signers,
    vec![
      "VGSTSESSION000000000".to_string(),
      "VGSTSESSION000000001".to_string()
    ]
  );
}

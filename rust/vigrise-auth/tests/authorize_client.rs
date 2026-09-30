#![cfg(feature = "verify")]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use axum::{Json, Router};
use serde_json::{json, Map, Value};
use vigrise_auth::verify::{AuthorizeClient, AuthorizeRequest, Authorizer, Decision};
use vigrise_auth::{AuthorizationHeader, Credentials};

#[derive(Clone)]
struct Script {
  calls: Arc<AtomicUsize>,
  /// Status returned per call index; calls past the end return 200 Allow.
  statuses: Arc<Vec<StatusCode>>,
}

async fn handler(
  State(script): State<Script>,
  headers: HeaderMap,
  Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
  let n = script.calls.fetch_add(1, Ordering::SeqCst);
  let auth = AuthorizationHeader::parse(headers["authorization"].to_str().unwrap()).unwrap();
  assert_eq!(auth.service, "iam");
  assert_eq!(auth.access_key_id, "VGAKSERVICE");
  assert_eq!(body["action"], "tts:SynthesizeSpeech");
  if let Some(status) = script.statuses.get(n) {
    return (*status, Json(json!({})));
  }
  (
    StatusCode::OK,
    Json(json!({
      "decision": "Allow",
      "principal": {
        "vrn": "vrn:vigrise:iam:p:user/u", "id": "00000000-0000-0000-0000-000000000000",
        "type": "User", "name": "u",
        "project_id": "00000000-0000-0000-0000-000000000000",
        "organization_id": "00000000-0000-0000-0000-000000000000"
      },
      "tier": "Pro"
    })),
  )
}

async fn serve(statuses: Vec<StatusCode>) -> (String, Arc<AtomicUsize>) {
  let calls = Arc::new(AtomicUsize::new(0));
  let script = Script {
    calls: calls.clone(),
    statuses: Arc::new(statuses),
  };
  let app = Router::new()
    .route("/api/internal/v1/authorize", post(handler))
    .with_state(script);
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let addr = listener.local_addr().unwrap();
  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  (format!("http://{addr}/api/internal/v1/authorize"), calls)
}

fn request() -> AuthorizeRequest {
  AuthorizeRequest {
    security_token: None,
    access_key_id: "VGAKUSER".into(),
    string_to_sign: "sts".into(),
    signature: "sig".into(),
    action: "tts:SynthesizeSpeech".into(),
    resources: vec![],
    context: Map::new(),
    usage: Default::default(),
    streaming_signing_key: false,
    presigned_expires: None,
  }
}

fn client(endpoint: String) -> AuthorizeClient {
  AuthorizeClient::new(endpoint, Credentials::new("VGAKSERVICE", "service-secret"))
}

#[tokio::test]
async fn signs_with_iam_scope_and_parses_allow() {
  let (endpoint, calls) = serve(vec![]).await;
  let response = client(endpoint).authorize(&request()).await.unwrap();
  assert_eq!(response.decision, Decision::Allow);
  assert_eq!(response.tier.as_deref(), Some("Pro"));
  assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn retries_once_on_server_error() {
  let (endpoint, calls) = serve(vec![StatusCode::BAD_GATEWAY]).await;
  let response = client(endpoint).authorize(&request()).await.unwrap();
  assert_eq!(response.decision, Decision::Allow);
  assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn two_server_errors_fail_closed() {
  let (endpoint, calls) = serve(vec![StatusCode::BAD_GATEWAY, StatusCode::BAD_GATEWAY]).await;
  let err = client(endpoint).authorize(&request()).await.unwrap_err();
  assert_eq!(err.code(), "ServiceUnavailable");
  assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn service_rejected_by_platform_does_not_retry() {
  let (endpoint, calls) = serve(vec![StatusCode::FORBIDDEN]).await;
  let err = client(endpoint).authorize(&request()).await.unwrap_err();
  assert_eq!(err.code(), "ServiceUnavailable");
  assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn release_posts_the_leases_signed_under_the_iam_scope() {
  let received: Arc<std::sync::Mutex<Vec<(String, Value)>>> = Arc::default();
  let recorder = received.clone();
  let app = Router::new().route(
    "/api/internal/v1/leases/release",
    post(move |headers: HeaderMap, Json(body): Json<Value>| {
      let recorder = recorder.clone();
      async move {
        let auth = AuthorizationHeader::parse(headers["authorization"].to_str().unwrap()).unwrap();
        recorder.lock().unwrap().push((auth.service, body));
        StatusCode::NO_CONTENT
      }
    }),
  );
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let addr = listener.local_addr().unwrap();
  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  let client = client(format!("http://{addr}/api/internal/v1/authorize"));
  client.release(vec!["p:tts:concurrent-syntheses:1".into()]);
  for _ in 0..200 {
    if !received.lock().unwrap().is_empty() {
      break;
    }
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
  }
  let received = received.lock().unwrap();
  assert_eq!(received.len(), 1);
  assert_eq!(received[0].0, "iam");
  assert_eq!(
    received[0].1,
    json!({ "leases": ["p:tts:concurrent-syntheses:1"] })
  );
}

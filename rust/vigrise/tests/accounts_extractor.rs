#![cfg(feature = "accounts-axum")]
mod common;

use std::sync::Arc;

use axum::{
  Json, Router,
  body::Body,
  extract::FromRef,
  http::{Request, StatusCode},
  routing::{get, post},
};
use serde_json::json;
use tower::ServiceExt;
use vigrise::accounts::{AccountsIdentity, SsoIntrospector, SsoIntrospectorConfig};

#[derive(Clone)]
struct AppState {
  introspector: Arc<SsoIntrospector>,
}

impl FromRef<AppState> for Arc<SsoIntrospector> {
  fn from_ref(state: &AppState) -> Self {
    state.introspector.clone()
  }
}

async fn who(identity: AccountsIdentity) -> String {
  format!(
    "{}:{}",
    identity.identity.email.as_deref().unwrap_or(""),
    identity.session.id
  )
}

async fn fake_introspect(Json(body): Json<serde_json::Value>) -> Json<serde_json::Value> {
  if body["token"] == "good" {
    Json(json!({"active": true,
            "identity": {"id": "i1", "email": "a@example.com", "name": "A", "picture": null, "email_verified": true},
            "session": {"id": "s1", "authenticated_at": "2026-10-11T10:00:00Z", "aal": "aal1", "expires_at": "2026-10-12T10:00:00Z"},
            "accounts": []}))
  } else {
    Json(json!({"active": false}))
  }
}

async fn app(url: &str) -> Router {
  let config = SsoIntrospectorConfig::new(url, "drive", "secret");
  let state = AppState {
    introspector: Arc::new(SsoIntrospector::new(config)),
  };
  Router::new().route("/who", get(who)).with_state(state)
}

async fn call(router: Router, header: Option<&str>) -> (StatusCode, String) {
  let mut request = Request::builder().uri("/who");
  if let Some(value) = header {
    request = request.header("x-vigrise-sso", value);
  }
  let response = router
    .oneshot(request.body(Body::empty()).unwrap())
    .await
    .unwrap();
  let status = response.status();
  let bytes = axum::body::to_bytes(response.into_body(), 4096)
    .await
    .unwrap();
  (status, String::from_utf8(bytes.to_vec()).unwrap())
}

#[tokio::test]
async fn the_extractor_maps_introspection_to_http_statuses() {
  let addr =
    common::serve(Router::new().route("/sessions/sso/introspect", post(fake_introspect))).await;
  let url = format!("http://{addr}");

  let (status, body) = call(app(&url).await, Some("good")).await;
  assert_eq!(
    (status, body.as_str()),
    (StatusCode::OK, "a@example.com:s1")
  );

  let (status, body) = call(app(&url).await, Some("stale")).await;
  assert_eq!(status, StatusCode::UNAUTHORIZED);
  assert!(body.contains("SsoSessionInactive"));

  let (status, _) = call(app(&url).await, None).await;
  assert_eq!(status, StatusCode::UNAUTHORIZED);

  let (status, body) = call(app("http://127.0.0.1:9").await, Some("good")).await;
  assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
  assert!(body.contains("AccountsUnavailable"));
}

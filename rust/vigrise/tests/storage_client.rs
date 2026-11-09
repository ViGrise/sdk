#![cfg(feature = "storage")]

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use chrono::{Duration, Utc};
use vigrise::auth::Credentials;
use vigrise::storage::{PresignRequest, StorageClient};

async fn storage_seeing_delegation() -> (String, Arc<Mutex<Vec<Option<String>>>>) {
  let seen: Arc<Mutex<Vec<Option<String>>>> = Arc::default();
  let recorder = seen.clone();
  let app = Router::new().fallback(move |request: Request| {
    let recorder = recorder.clone();
    async move {
      let token = request
        .headers()
        .get("x-vigrise-delegation")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
      recorder.lock().unwrap().push(token);
      (
        StatusCode::OK,
        [("content-type", "application/json")],
        r#"{"url":"https://storage.example/o","expires_at":"2026-10-01T12:00:00Z"}"#,
      )
        .into_response()
    }
  });
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let url = format!("http://{}", listener.local_addr().unwrap());
  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
  (url, seen)
}

fn request() -> PresignRequest {
  PresignRequest {
    key: "a.txt".into(),
    method: "GET".into(),
    expires_in: 600,
    ..Default::default()
  }
}

#[tokio::test]
async fn presign_carries_the_delegation_token_past_the_deadline() {
  let (url, seen) = storage_seeing_delegation().await;
  let client = StorageClient::new(url, Arc::new(Credentials::new("VGAKTEST", "secret")));
  let expired = client
    .delegated("person-token", Some(Utc::now() - Duration::minutes(1)))
    .unwrap();

  expired.presign("photos", &request()).await.unwrap();
  expired
    .presign_delegated("photos", &request(), None)
    .await
    .unwrap();

  assert_eq!(
    *seen.lock().unwrap(),
    vec![
      Some("person-token".to_owned()),
      Some("person-token".to_owned())
    ]
  );
}

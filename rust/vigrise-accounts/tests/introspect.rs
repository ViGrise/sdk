mod common;

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

use axum::{extract::State, http::HeaderMap, http::StatusCode, routing::post, Json, Router};
use serde_json::{json, Value};
use vigrise_accounts::{sso_token_hash, AccountsError, SsoIntrospector, SsoIntrospectorConfig};

#[derive(Clone)]
struct Fake {
    calls: Arc<AtomicUsize>,
    status: StatusCode,
    active: bool,
}

async fn introspect(
    State(fake): State<Fake>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
    fake.calls.fetch_add(1, Ordering::SeqCst);
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    // base64("drive:secret")
    if auth != "Basic ZHJpdmU6c2VjcmV0" {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "invalid_client"})),
        );
    }
    if fake.status != StatusCode::OK {
        return (fake.status, Json(json!({"error": "x"})));
    }
    if !fake.active || body["token"] != "good-token" {
        return (StatusCode::OK, Json(json!({"active": false})));
    }
    (
        StatusCode::OK,
        Json(json!({
            "active": true,
            "identity": {"id": "11111111-1111-1111-1111-111111111111", "email": "a@example.com",
                "name": "Ada", "picture": null, "email_verified": true},
            "session": {"id": "sid-1", "authenticated_at": "2026-10-11T10:00:00Z",
                "aal": "aal1", "expires_at": "2026-10-12T10:00:00Z"},
            "accounts": [{"session_id": "sid-1", "identity_id": "11111111-1111-1111-1111-111111111111",
                "name": "Ada", "email": "a@example.com", "picture": null, "active": true, "expired": false}]
        })),
    )
}

async fn start(status: StatusCode, active: bool) -> (SsoIntrospector, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let app = Router::new()
        .route("/sessions/sso/introspect", post(introspect))
        .with_state(Fake {
            calls: calls.clone(),
            status,
            active,
        });
    let addr = common::serve(app).await;
    let config = SsoIntrospectorConfig::new(format!("http://{addr}"), "drive", "secret");
    (SsoIntrospector::new(config), calls)
}

#[tokio::test]
async fn an_active_token_returns_identity_session_and_accounts() {
    let (introspector, _) = start(StatusCode::OK, true).await;
    let answer = introspector.introspect("good-token").await.unwrap();
    assert!(answer.active);
    assert_eq!(answer.identity.unwrap().email, "a@example.com");
    assert_eq!(answer.session.unwrap().id, "sid-1");
    assert_eq!(answer.accounts.len(), 1);
    assert!(answer.accounts[0].active);
}

#[tokio::test]
async fn an_unknown_token_is_inactive() {
    let (introspector, _) = start(StatusCode::OK, true).await;
    let answer = introspector.introspect("other-token").await.unwrap();
    assert!(!answer.active);
    assert!(answer.identity.is_none());
}

#[tokio::test]
async fn answers_are_cached_per_token_until_invalidated() {
    let (introspector, calls) = start(StatusCode::OK, true).await;
    introspector.introspect("good-token").await.unwrap();
    introspector.introspect("good-token").await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    introspector.invalidate_session("sid-1");
    introspector.introspect("good-token").await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    introspector.invalidate_token(&sso_token_hash("good-token"));
    introspector.introspect("good-token").await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn cache_entries_expire_after_the_ttl() {
    let (introspector, calls) = start(StatusCode::OK, true).await;
    let introspector = introspector.with_cache_ttl(Duration::from_millis(30));
    introspector.introspect("good-token").await.unwrap();
    tokio::time::sleep(Duration::from_millis(60)).await;
    introspector.introspect("good-token").await.unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn client_errors_map_to_typed_errors_and_are_not_cached() {
    let (forbidden, calls) = start(StatusCode::FORBIDDEN, true).await;
    assert!(matches!(
        forbidden.introspect("good-token").await,
        Err(AccountsError::Forbidden)
    ));
    assert!(matches!(
        forbidden.introspect("good-token").await,
        Err(AccountsError::Forbidden)
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 2);

    let (limited, _) = start(StatusCode::TOO_MANY_REQUESTS, true).await;
    assert!(matches!(
        limited.introspect("good-token").await,
        Err(AccountsError::RateLimited)
    ));

    let (down, _) = start(StatusCode::BAD_GATEWAY, true).await;
    assert!(matches!(
        down.introspect("good-token").await,
        Err(AccountsError::Unavailable(_))
    ));
}

#[tokio::test]
async fn wrong_credentials_are_invalid_client() {
    let (introspector, _) = start(StatusCode::OK, true).await;
    let wrong = SsoIntrospector::new(SsoIntrospectorConfig::new(
        introspector.internal_url().to_string(),
        "drive",
        "wrong",
    ));
    assert!(matches!(
        wrong.introspect("good-token").await,
        Err(AccountsError::InvalidClient)
    ));
}

#[tokio::test]
async fn an_unreachable_server_is_unavailable() {
    let unreachable = SsoIntrospector::new(SsoIntrospectorConfig::new(
        "http://127.0.0.1:9",
        "drive",
        "secret",
    ));
    assert!(matches!(
        unreachable.introspect("t").await,
        Err(AccountsError::Unavailable(_))
    ));
}

#[test]
fn the_token_hash_is_lowercase_sha256_hex() {
    assert_eq!(
        sso_token_hash("abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

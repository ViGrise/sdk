#![cfg(feature = "verify")]

use std::sync::Arc;

use axum::body::Body;
use axum::extract::Request;
use axum::routing::put;
use axum::{Extension, Router};
use bytes::Bytes;
use chrono::Utc;
use http::request::Parts;
use http::{Method, StatusCode, Uri};
use http_body_util::BodyExt;
use tower::ServiceExt;
use uuid::Uuid;
use vigrise::auth::signing::{derive_signing_key, parse_string_to_sign, verify_signature};
use vigrise::auth::verify::{
  verify, verify_streaming, ActionResolver, AuthorizeRequest, AuthorizeResponse, Authorized,
  Authorizer, Decision, ErrorBody, Principal, ResolvedAction, Verifier,
};
use vigrise::auth::{
  sign_request, sign_request_with_payload_hash, AuthError, Credentials, UNSIGNED_PAYLOAD,
};

const SECRET: &str = "test-secret";
const KEY_ID: &str = "VGAKTESTKEY";

/// Stands in for the platform: allows any request whose signature verifies.
struct SigningPlatform;

impl Authorizer for SigningPlatform {
  fn authorize<'a>(
    &'a self,
    request: &'a AuthorizeRequest,
  ) -> vigrise::auth::verify::AuthorizeFuture<'a> {
    Box::pin(async move {
      let parsed = parse_string_to_sign(&request.string_to_sign).unwrap();
      let key = derive_signing_key(SECRET, &parsed.date, &parsed.service);
      if !verify_signature(&key, &request.string_to_sign, &request.signature) {
        return Ok(AuthorizeResponse {
          decision: Decision::Deny,
          error: Some(ErrorBody {
            code: "SignatureDoesNotMatch".into(),
            message: "SignatureDoesNotMatch".into(),
          }),
          principal: None,
          tier: None,
          quotas: vec![],
          leases: vec![],
          signing_key: None,
        });
      }
      Ok(AuthorizeResponse {
        decision: Decision::Allow,
        error: None,
        principal: Some(Principal {
          vrn: "vrn:vigrise:iam:p:user/uploader".into(),
          id: Uuid::nil(),
          principal_type: "User".into(),
          name: "uploader".into(),
          project_id: Uuid::nil(),
          organization_id: Uuid::nil(),
          source_identity: None,
          source_principal: None,
        }),
        tier: Some("Pro".into()),
        quotas: vec![],
        leases: vec![],
        signing_key: None,
      })
    })
  }
}

/// Records whether the resolver saw a body, which streamed routes never pass.
struct PutResolver;

impl ActionResolver for PutResolver {
  fn resolve(&self, _parts: &Parts, body: &Bytes) -> Result<ResolvedAction, AuthError> {
    assert!(body.is_empty(), "streamed routes resolve without the body");
    Ok(ResolvedAction {
      action: "storage:PutObject".into(),
      resources: vec!["vrn:vigrise:storage:${ProjectId}:bucket/b/k".into()],
      ..Default::default()
    })
  }
}

fn verifier() -> Arc<Verifier> {
  Arc::new(Verifier {
    service: "storage".into(),
    authorizer: Arc::new(SigningPlatform),
    resolver: Arc::new(PutResolver),
    max_body_bytes: 16,
    trust_forwarded_headers: false,
  })
}

/// The handler reads the whole body and answers how many bytes arrived, or
/// 422 when reading the body failed.
fn app(streaming: bool) -> Router {
  let handler = |Extension(auth): Extension<Authorized>, request: Request| async move {
    match request.into_body().collect().await {
      Ok(collected) => (
        StatusCode::OK,
        format!("{}:{}", auth.principal.name, collected.to_bytes().len()),
      ),
      Err(error) => (StatusCode::UNPROCESSABLE_ENTITY, error.to_string()),
    }
  };
  let routes = Router::new().route("/v1/buckets/b/objects/k", put(handler));
  if streaming {
    routes.layer(axum::middleware::from_fn_with_state(
      verifier(),
      verify_streaming,
    ))
  } else {
    routes.layer(axum::middleware::from_fn_with_state(verifier(), verify))
  }
}

fn request(payload_hash: Option<&str>, body: &'static [u8]) -> Request {
  let uri: Uri = "http://storage.test/v1/buckets/b/objects/k"
    .parse()
    .unwrap();
  let headers = http::HeaderMap::new();
  let credentials = Credentials::new(KEY_ID, SECRET);
  let signed = match payload_hash {
    Some(hash) => sign_request_with_payload_hash(
      &Method::PUT,
      &uri,
      &headers,
      hash,
      &credentials,
      "storage",
      Utc::now(),
    ),
    None => sign_request(
      &Method::PUT,
      &uri,
      &headers,
      body,
      &credentials,
      "storage",
      Utc::now(),
    ),
  }
  .unwrap();
  let mut request = Request::builder()
    .method(Method::PUT)
    .uri("/v1/buckets/b/objects/k")
    .header("host", "storage.test")
    .body(Body::from(body))
    .unwrap();
  signed.apply(request.headers_mut()).unwrap();
  request
}

async fn send(app: Router, request: Request) -> (StatusCode, String) {
  let response = app.oneshot(request).await.unwrap();
  let status = response.status();
  let bytes = response.into_body().collect().await.unwrap().to_bytes();
  (status, String::from_utf8_lossy(&bytes).to_string())
}

const LARGE: &[u8] = &[7u8; 64 * 1024];

#[tokio::test]
async fn an_unsigned_payload_streams_past_the_body_limit() {
  let (status, text) = send(app(true), request(Some(UNSIGNED_PAYLOAD), LARGE)).await;
  assert_eq!(status, StatusCode::OK, "{text}");
  assert_eq!(text, format!("uploader:{}", LARGE.len()));
}

#[tokio::test]
async fn a_signed_payload_hash_is_checked_while_streaming() {
  let (status, text) = send(app(true), request(None, LARGE)).await;
  assert_eq!(status, StatusCode::OK, "{text}");
  assert_eq!(text, format!("uploader:{}", LARGE.len()));
}

#[tokio::test]
async fn a_body_that_does_not_match_its_hash_fails_at_its_end() {
  let other_hash = vigrise::auth::canonical::hex_sha256(b"something else");
  let (status, text) = send(app(true), request(Some(&other_hash), LARGE)).await;
  assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{text}");
  assert!(text.contains("x-vgr-content-sha256"), "{text}");
}

#[tokio::test]
async fn a_malformed_payload_hash_is_refused_before_the_handler() {
  let (status, text) = send(app(true), request(Some("not-a-hash"), LARGE)).await;
  assert_eq!(status, StatusCode::UNAUTHORIZED, "{text}");
  assert!(text.contains("SignatureDoesNotMatch"), "{text}");
}

#[tokio::test]
async fn buffered_routes_still_refuse_an_unsigned_payload() {
  let (status, text) = send(app(false), request(Some(UNSIGNED_PAYLOAD), b"tiny")).await;
  assert_eq!(status, StatusCode::UNAUTHORIZED, "{text}");
  assert!(text.contains("SignatureDoesNotMatch"), "{text}");
}

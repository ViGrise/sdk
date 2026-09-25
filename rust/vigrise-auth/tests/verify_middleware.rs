#![cfg(feature = "verify")]

use std::sync::Arc;

use axum::body::Body;
use axum::extract::Request;
use axum::routing::post;
use axum::{Extension, Router};
use bytes::Bytes;
use chrono::Utc;
use http::request::Parts;
use http::{Method, StatusCode, Uri};
use http_body_util::BodyExt;
use serde_json::{json, Map, Value};
use tower::ServiceExt;
use uuid::Uuid;
use vigrise_auth::signing::{derive_signing_key, parse_string_to_sign, verify_signature};
use vigrise_auth::verify::{
  verify, ActionResolver, AuthorizeRequest, AuthorizeResponse, Authorized, Authorizer, Decision,
  ErrorBody, Principal, ResolvedAction, Verifier,
};
use vigrise_auth::{sign_request, AuthError, Credentials};

const SECRET: &str = "test-secret";
const KEY_ID: &str = "VGAKTESTKEY";

/// Stands in for the platform: verifies the signature with the known secret and
/// allows only `tts:SynthesizeSpeech`.
struct FakePlatform {
  available: bool,
  omit_principal: bool,
}

impl Authorizer for FakePlatform {
  fn authorize<'a>(
    &'a self,
    request: &'a AuthorizeRequest,
  ) -> vigrise_auth::verify::AuthorizeFuture<'a> {
    Box::pin(async move {
      if !self.available {
        return Err(AuthError::ServiceUnavailable(
          "Authorization is unavailable".into(),
        ));
      }
      let deny = |code: &str| AuthorizeResponse {
        decision: Decision::Deny,
        error: Some(ErrorBody {
          code: code.into(),
          message: code.into(),
        }),
        principal: None,
        project_plan: None,
      };
      let parsed = parse_string_to_sign(&request.string_to_sign).unwrap();
      let key = derive_signing_key(SECRET, &parsed.date, &parsed.service);
      if request.access_key_id != KEY_ID {
        return Ok(deny("InvalidClientTokenId"));
      }
      if !verify_signature(&key, &request.string_to_sign, &request.signature) {
        return Ok(deny("SignatureDoesNotMatch"));
      }
      if request.action != "tts:SynthesizeSpeech" {
        return Ok(deny("AccessDenied"));
      }
      if self.omit_principal {
        return Ok(AuthorizeResponse {
          decision: Decision::Allow,
          error: None,
          principal: None,
          project_plan: None,
        });
      }
      Ok(AuthorizeResponse {
        decision: Decision::Allow,
        error: None,
        principal: Some(Principal {
          vrn: "vrn:vigrise:iam:p:user/ci-bot".into(),
          id: Uuid::nil(),
          principal_type: "User".into(),
          name: "ci-bot".into(),
          project_id: Uuid::nil(),
          organization_id: Uuid::nil(),
        }),
        project_plan: Some("Pro".into()),
      })
    })
  }
}

struct RouteResolver;

impl ActionResolver for RouteResolver {
  fn resolve(&self, parts: &Parts, body: &Bytes) -> Result<ResolvedAction, AuthError> {
    let path = parts
      .extensions
      .get::<axum::extract::OriginalUri>()
      .map(|u| u.0.path().to_string())
      .unwrap_or_else(|| parts.uri.path().to_string());
    let action = match path.as_str() {
      "/v1/audio/speech" | "/api/v1/audio/speech" => "tts:SynthesizeSpeech",
      "/v1/voices" => "tts:ListVoices",
      _ => {
        return Err(AuthError::Rejected {
          status: StatusCode::NOT_FOUND,
          code: "UnknownOperation".into(),
          message: "Unknown operation".into(),
        })
      }
    };
    let voice = serde_json::from_slice::<Value>(body)
      .ok()
      .and_then(|v| v.get("voice").and_then(Value::as_str).map(str::to_string))
      .unwrap_or_default();
    let mut context = Map::new();
    context.insert("tts:VoiceId".into(), json!(voice));
    Ok(ResolvedAction {
      action: action.into(),
      resources: vec![format!("vrn:vigrise:tts:${{ProjectId}}:voice/{voice}")],
      context,
    })
  }
}

fn app(available: bool, nested: bool) -> Router {
  app_with(
    FakePlatform {
      available,
      omit_principal: false,
    },
    nested,
  )
}

fn app_with(platform: FakePlatform, nested: bool) -> Router {
  let verifier = Arc::new(Verifier {
    service: "tts".into(),
    authorizer: Arc::new(platform),
    resolver: Arc::new(RouteResolver),
    max_body_bytes: 1024,
    trust_forwarded_headers: false,
  });
  let handler = |Extension(auth): Extension<Authorized>, body: Bytes| async move {
    format!("{}:{}", auth.principal.name, String::from_utf8_lossy(&body))
  };
  let routes = Router::new()
    .route("/v1/audio/speech", post(handler))
    .route("/v1/voices", post(handler))
    .layer(axum::middleware::from_fn_with_state(verifier, verify));
  if nested {
    Router::new().nest("/api", routes)
  } else {
    routes
  }
}

fn signed(path: &str, body: &str, secret: &str) -> Request {
  let uri: Uri = format!("http://tts.test{path}").parse().unwrap();
  let mut headers = http::HeaderMap::new();
  headers.insert("content-type", "application/json".parse().unwrap());
  let sig = sign_request(
    &Method::POST,
    &uri,
    &headers,
    body.as_bytes(),
    &Credentials::new(KEY_ID, secret),
    "tts",
    Utc::now(),
  )
  .unwrap();
  let mut request = Request::builder()
    .method(Method::POST)
    .uri(path)
    .header("host", "tts.test")
    .header("content-type", "application/json")
    .body(Body::from(body.to_string()))
    .unwrap();
  sig.apply(request.headers_mut()).unwrap();
  request
}

async fn send(app: Router, request: Request) -> (StatusCode, Value, String) {
  let response = app.oneshot(request).await.unwrap();
  let status = response.status();
  let bytes = response.into_body().collect().await.unwrap().to_bytes();
  let text = String::from_utf8_lossy(&bytes).to_string();
  (
    status,
    serde_json::from_str(&text).unwrap_or(Value::Null),
    text,
  )
}

const BODY: &str = r#"{"voice":"haley-f1","input":"xin chao"}"#;

#[tokio::test]
async fn allowed_request_reaches_handler_with_principal_and_body() {
  let (status, _, text) = send(app(true, false), signed("/v1/audio/speech", BODY, SECRET)).await;
  assert_eq!(status, StatusCode::OK);
  assert_eq!(text, format!("ci-bot:{BODY}"));
}

#[tokio::test]
async fn nested_router_verifies_against_full_path() {
  let (status, _, _) = send(
    app(true, true),
    signed("/api/v1/audio/speech", BODY, SECRET),
  )
  .await;
  assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn missing_authorization_is_401() {
  let request = Request::post("/v1/audio/speech")
    .body(Body::from(BODY))
    .unwrap();
  let (status, json, _) = send(app(true, false), request).await;
  assert_eq!(status, StatusCode::UNAUTHORIZED);
  assert_eq!(json["error"]["code"], "MissingAuthenticationToken");
}

#[tokio::test]
async fn tampered_body_is_rejected() {
  let mut request = signed("/v1/audio/speech", BODY, SECRET);
  *request.body_mut() = Body::from(BODY.replace("haley-f1", "other"));
  let (status, json, _) = send(app(true, false), request).await;
  assert_eq!(status, StatusCode::UNAUTHORIZED);
  assert_eq!(json["error"]["code"], "SignatureDoesNotMatch");
}

#[tokio::test]
async fn wrong_secret_returns_string_to_sign() {
  let (status, json, _) = send(app(true, false), signed("/v1/audio/speech", BODY, "wrong")).await;
  assert_eq!(status, StatusCode::UNAUTHORIZED);
  assert_eq!(json["error"]["code"], "SignatureDoesNotMatch");
  assert!(json["error"]["string_to_sign"]
    .as_str()
    .unwrap()
    .starts_with("VGR4-HMAC-SHA256\n"));
}

#[tokio::test]
async fn denied_action_is_403() {
  let (status, json, _) = send(app(true, false), signed("/v1/voices", BODY, SECRET)).await;
  assert_eq!(status, StatusCode::FORBIDDEN);
  assert_eq!(json["error"]["code"], "AccessDenied");
}

#[tokio::test]
async fn unavailable_platform_fails_closed() {
  let (status, json, _) = send(app(false, false), signed("/v1/audio/speech", BODY, SECRET)).await;
  assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
  assert_eq!(json["error"]["code"], "ServiceUnavailable");
}

#[tokio::test]
async fn oversized_body_is_413() {
  let big = format!(r#"{{"voice":"v","input":"{}"}}"#, "x".repeat(2048));
  let (status, _, _) = send(app(true, false), signed("/v1/audio/speech", &big, SECRET)).await;
  assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn host_rewritten_by_proxy_fails_with_string_to_sign() {
  let mut request = signed("/v1/audio/speech", BODY, SECRET);
  request
    .headers_mut()
    .insert("host", "internal:8080".parse().unwrap());
  let (status, json, _) = send(app(true, false), request).await;
  assert_eq!(status, StatusCode::UNAUTHORIZED);
  assert_eq!(json["error"]["code"], "SignatureDoesNotMatch");
  assert!(json["error"]["string_to_sign"].is_string());
}

#[tokio::test]
async fn allow_without_principal_fails_closed() {
  let platform = FakePlatform {
    available: true,
    omit_principal: true,
  };
  let (status, json, _) = send(
    app_with(platform, false),
    signed("/v1/audio/speech", BODY, SECRET),
  )
  .await;
  assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
  assert_eq!(json["error"]["code"], "ServiceUnavailable");
}

/// HTTP/2 carries the host in `:authority`, which servers expose as the URI
/// authority with no `Host` header.
#[tokio::test]
async fn http2_request_without_host_header_verifies_from_authority() {
  let mut request = signed("/v1/audio/speech", BODY, SECRET);
  request.headers_mut().remove("host");
  *request.uri_mut() = "http://tts.test/v1/audio/speech".parse().unwrap();
  let (status, json, _) = send(app(true, false), request).await;
  assert_eq!(status, StatusCode::OK, "{json}");
}

/// Allows everything and records the context it was asked about.
struct CaptureContext {
  seen: std::sync::Mutex<Option<Map<String, Value>>>,
}

impl Authorizer for CaptureContext {
  fn authorize<'a>(
    &'a self,
    request: &'a AuthorizeRequest,
  ) -> vigrise_auth::verify::AuthorizeFuture<'a> {
    *self.seen.lock().unwrap() = Some(request.context.clone());
    Box::pin(async move {
      Ok(AuthorizeResponse {
        decision: Decision::Allow,
        error: None,
        principal: Some(Principal {
          vrn: "vrn:vigrise:iam:p:user/u".into(),
          id: Uuid::nil(),
          principal_type: "User".into(),
          name: "u".into(),
          project_id: Uuid::nil(),
          organization_id: Uuid::nil(),
        }),
        project_plan: None,
      })
    })
  }
}

/// Appending proxies (nginx `$proxy_add_x_forwarded_for`, cloud load
/// balancers) put the client-supplied value first and the address they saw last.
#[tokio::test]
async fn forwarded_source_ip_is_the_entry_added_by_the_trusted_proxy() {
  let capture = Arc::new(CaptureContext {
    seen: std::sync::Mutex::new(None),
  });
  let verifier = Arc::new(Verifier {
    service: "tts".into(),
    authorizer: capture.clone(),
    resolver: Arc::new(RouteResolver),
    max_body_bytes: 1024,
    trust_forwarded_headers: true,
  });
  let app = Router::new()
    .route("/v1/audio/speech", post(|| async { "ok" }))
    .layer(axum::middleware::from_fn_with_state(verifier, verify));
  let mut request = signed("/v1/audio/speech", BODY, SECRET);
  request
    .headers_mut()
    .insert("x-forwarded-for", "6.6.6.6, 203.0.113.9".parse().unwrap());
  let (status, _, _) = send(app, request).await;
  assert_eq!(status, StatusCode::OK);
  let seen = capture.seen.lock().unwrap().clone().unwrap();
  assert_eq!(seen["vgr:SourceIp"], "203.0.113.9");
}

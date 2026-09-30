use std::borrow::Cow;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{ConnectInfo, OriginalUri, Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use http::request::Parts;
use http::{header, HeaderMap, HeaderValue, Uri};
use http_body_util::{BodyExt, LengthLimitError, Limited};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::authorization::AuthorizationHeader;
use crate::canonical::{canonical_request, hex_sha256};
use crate::error::AuthError;
use crate::signer::{UNSIGNED_PAYLOAD, X_VGR_CONTENT_SHA256, X_VGR_DATE};
use crate::signing::string_to_sign;
use crate::verify::client::Authorizer;
use crate::verify::resolver::{ActionResolver, ResolvedAction};
use crate::verify::types::{AuthorizeRequest, AuthorizeResponse, Authorized, Decision, Principal};
use crate::QuotaStatus;

pub struct Verifier {
  /// Catalog namespace this service owns, for example `tts`.
  pub service: String,
  pub authorizer: Arc<dyn Authorizer>,
  pub resolver: Arc<dyn ActionResolver>,
  pub max_body_bytes: usize,
  /// Take client IP and scheme from `x-forwarded-for` / `x-forwarded-proto`.
  /// The source IP is the rightmost `x-forwarded-for` entry, the one the
  /// directly connected proxy appended. Enable only when exactly one trusted
  /// proxy sits in front of the service and clients cannot reach it directly.
  pub trust_forwarded_headers: bool,
}

impl IntoResponse for AuthError {
  fn into_response(self) -> Response {
    let mut error = json!({ "code": self.code(), "message": self.to_string() });
    if let AuthError::SignatureDoesNotMatch {
      string_to_sign: Some(sts),
      ..
    } = &self
    {
      error["string_to_sign"] = Value::String(sts.clone());
    }
    if let Some(quota) = self.quota() {
      error["quota"] = serde_json::to_value(quota).expect("quota status serializes");
    }
    (self.status(), axum::Json(json!({ "error": error }))).into_response()
  }
}

/// axum middleware: `.layer(axum::middleware::from_fn_with_state(verifier, verify))`.
pub async fn verify(
  State(verifier): State<Arc<Verifier>>,
  request: Request,
  next: Next,
) -> Response {
  let (parts, body) = request.into_parts();
  let body = match Limited::new(body, verifier.max_body_bytes).collect().await {
    Ok(collected) => collected.to_bytes(),
    Err(e) if e.downcast_ref::<LengthLimitError>().is_some() => {
      return AuthError::RequestEntityTooLarge(format!(
        "Request body exceeds {} bytes",
        verifier.max_body_bytes
      ))
      .into_response();
    }
    Err(_) => {
      return AuthError::Rejected {
        status: http::StatusCode::BAD_REQUEST,
        code: "InvalidRequest".into(),
        message: "Request body could not be read".into(),
      }
      .into_response();
    }
  };

  match authorize(&verifier, &parts, Payload::Buffered(&body)).await {
    Ok(authorized) => run(&verifier, parts, authorized, Body::from(body), next).await,
    Err(error) => error.into_response(),
  }
}

/// axum middleware for routes whose request bodies stream, such as uploads:
/// the body is never buffered. `x-vgr-content-sha256` is either
/// `UNSIGNED-PAYLOAD` or the body's hex SHA-256; a signed hash is checked as
/// the body streams, and a body that does not match it fails at its end, so a
/// handler that reads the body to the end never accepts it. The resolver sees
/// an empty body.
pub async fn verify_streaming(
  State(verifier): State<Arc<Verifier>>,
  request: Request,
  next: Next,
) -> Response {
  let (parts, body) = request.into_parts();
  let payload_hash = match header_str(&parts, X_VGR_CONTENT_SHA256) {
    Some(UNSIGNED_PAYLOAD) => None,
    Some(hash)
      if hash.len() == 64 && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) =>
    {
      Some(hash.to_string())
    }
    _ => {
      return AuthError::SignatureDoesNotMatch {
        message: "x-vgr-content-sha256 must be UNSIGNED-PAYLOAD or a hex SHA-256".into(),
        string_to_sign: None,
      }
      .into_response()
    }
  };
  let signed_hash = payload_hash.as_deref().unwrap_or(UNSIGNED_PAYLOAD);
  match authorize(&verifier, &parts, Payload::Streamed(signed_hash)).await {
    Ok(authorized) => {
      let body = match payload_hash {
        Some(expected) => Body::new(HashCheckedBody {
          inner: body,
          hasher: Some(Sha256::new()),
          expected,
        }),
        None => body,
      };
      run(&verifier, parts, authorized, body, next).await
    }
    Err(error) => error.into_response(),
  }
}

/// Passes the authorized request on, holding its concurrency slots until the
/// response body ends.
async fn run(
  verifier: &Verifier,
  mut parts: Parts,
  authorized: Authorized,
  body: Body,
  next: Next,
) -> Response {
  // Held from the moment the slots exist, so a cancelled request or a
  // panicking handler returns them as well as a finished response.
  let guard = (!authorized.leases.is_empty()).then(|| ReleaseGuard {
    authorizer: Arc::clone(&verifier.authorizer),
    leases: authorized.leases.clone(),
  });
  parts.extensions.insert(authorized);
  let response = next.run(Request::from_parts(parts, body)).await;
  match guard {
    Some(guard) => release_with(response, guard),
    None => response,
  }
}

/// Wraps `response` so the concurrency slots `leases` are returned once its
/// body has been sent or is dropped. Services that authorize requests
/// themselves use this for the leases their decisions hold.
pub fn hold_leases(
  response: Response,
  authorizer: Arc<dyn Authorizer>,
  leases: Vec<String>,
) -> Response {
  if leases.is_empty() {
    return response;
  }
  release_with(response, ReleaseGuard { authorizer, leases })
}

fn release_with(response: Response, guard: ReleaseGuard) -> Response {
  response.map(|body| {
    Body::new(ReleasingBody {
      inner: body,
      guard: Some(guard),
    })
  })
}

/// A request body that hashes what passes through it and fails with an error
/// at its end when the hash differs from the signed one.
struct HashCheckedBody {
  inner: Body,
  hasher: Option<Sha256>,
  expected: String,
}

impl http_body::Body for HashCheckedBody {
  type Data = Bytes;
  type Error = axum::Error;

  fn poll_frame(
    mut self: std::pin::Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
  ) -> std::task::Poll<Option<Result<http_body::Frame<Bytes>, Self::Error>>> {
    let polled = std::pin::Pin::new(&mut self.inner).poll_frame(cx);
    match &polled {
      std::task::Poll::Ready(Some(Ok(frame))) => {
        if let (Some(data), Some(hasher)) = (frame.data_ref(), self.hasher.as_mut()) {
          hasher.update(data);
        }
      }
      std::task::Poll::Ready(None) => {
        if let Some(hasher) = self.hasher.take() {
          if hex::encode(hasher.finalize()) != self.expected {
            return std::task::Poll::Ready(Some(Err(axum::Error::new(
              crate::signing::PayloadHashMismatch {
                algorithm: crate::signing::SigningAlgorithm::Vgr4,
              },
            ))));
          }
        }
      }
      _ => {}
    }
    polled
  }

  fn is_end_stream(&self) -> bool {
    self.hasher.is_none() && self.inner.is_end_stream()
  }

  fn size_hint(&self) -> http_body::SizeHint {
    self.inner.size_hint()
  }
}

/// Returns the request's concurrency slots exactly once, when dropped.
struct ReleaseGuard {
  authorizer: Arc<dyn Authorizer>,
  leases: Vec<String>,
}

impl Drop for ReleaseGuard {
  fn drop(&mut self) {
    self.authorizer.release(std::mem::take(&mut self.leases));
  }
}

/// A response body that releases the request's leases when it has been sent
/// completely or is dropped, for example when the client disconnects mid-stream.
struct ReleasingBody {
  inner: Body,
  guard: Option<ReleaseGuard>,
}

impl http_body::Body for ReleasingBody {
  type Data = Bytes;
  type Error = axum::Error;

  fn poll_frame(
    mut self: std::pin::Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
  ) -> std::task::Poll<Option<Result<http_body::Frame<Bytes>, Self::Error>>> {
    let polled = std::pin::Pin::new(&mut self.inner).poll_frame(cx);
    if let std::task::Poll::Ready(None) = polled {
      self.guard.take();
    }
    polled
  }

  fn is_end_stream(&self) -> bool {
    self.inner.is_end_stream()
  }

  fn size_hint(&self) -> http_body::SizeHint {
    self.inner.size_hint()
  }
}

fn header_str<'a>(parts: &'a Parts, name: &str) -> Option<&'a str> {
  parts.headers.get(name).and_then(|v| v.to_str().ok())
}

/// The body a request was signed over: read in full, or streaming with the
/// payload hash its signature names.
enum Payload<'a> {
  Buffered(&'a Bytes),
  Streamed(&'a str),
}

async fn authorize(
  verifier: &Verifier,
  parts: &Parts,
  payload: Payload<'_>,
) -> Result<Authorized, AuthError> {
  let authorization = header_str(parts, header::AUTHORIZATION.as_str())
    .ok_or(AuthError::MissingAuthenticationToken)?;
  let auth = AuthorizationHeader::parse(authorization)?;
  if auth.service != verifier.service {
    return Err(AuthError::IncompleteSignature(format!(
      "Credential scope service must be {}",
      verifier.service
    )));
  }

  let timestamp = header_str(parts, X_VGR_DATE)
    .ok_or_else(|| AuthError::IncompleteSignature("Missing x-vgr-date header".into()))?;
  if !timestamp.starts_with(&auth.date) {
    return Err(AuthError::IncompleteSignature(
      "Credential scope date must match x-vgr-date".into(),
    ));
  }

  let empty = Bytes::new();
  let (payload_hash, body) = match payload {
    Payload::Buffered(body) => {
      let payload_hash = hex_sha256(body);
      if header_str(parts, X_VGR_CONTENT_SHA256) != Some(payload_hash.as_str()) {
        return Err(AuthError::SignatureDoesNotMatch {
          message: "x-vgr-content-sha256 does not match the request body".into(),
          string_to_sign: None,
        });
      }
      (payload_hash, body)
    }
    Payload::Streamed(payload_hash) => (payload_hash.to_string(), &empty),
  };

  // Nested axum routers strip their prefix from `parts.uri`; the client signed the full path.
  let uri = parts
    .extensions
    .get::<OriginalUri>()
    .map(|original| original.0.clone())
    .unwrap_or_else(|| parts.uri.clone());
  let headers = headers_with_host(&parts.headers, &uri)?;
  let canonical = canonical_request(
    parts.method.as_str(),
    uri.path(),
    uri.query().unwrap_or(""),
    &headers,
    &auth.signed_headers,
    &payload_hash,
  )?;
  let sts = string_to_sign(timestamp, &auth.scope(), &canonical);

  let resolved = verifier.resolver.resolve(parts, body)?;
  let additional = verifier.resolver.additional(parts, body)?;
  let global = global_context(verifier, parts);
  let build = |resolved: ResolvedAction| {
    let mut context = resolved.context;
    for (key, value) in &global {
      context.insert(key.to_string(), value.clone());
    }
    AuthorizeRequest {
      access_key_id: auth.access_key_id.clone(),
      security_token: header_str(parts, "x-vgr-security-token").map(str::to_string),
      string_to_sign: sts.clone(),
      signature: auth.signature.clone(),
      action: resolved.action,
      resources: resolved.resources,
      context,
      usage: resolved.usage,
      streaming_signing_key: false,
      presigned_expires: None,
    }
  };
  let request = build(resolved);
  let usage = request.usage.clone();
  let mut allowed = decide(verifier, &request, &sts).await?;
  for extra in additional {
    match decide(verifier, &build(extra), &sts).await {
      Ok(extra_allowed) => {
        allowed.leases.extend(extra_allowed.leases);
        allowed.quotas.extend(extra_allowed.quotas);
      }
      Err(error) => {
        verifier.authorizer.release(allowed.leases);
        return Err(error);
      }
    }
  }
  Ok(Authorized {
    principal: allowed.principal,
    tier: allowed.tier,
    quotas: allowed.quotas,
    usage,
    leases: allowed.leases,
    signing_key: allowed.signing_key,
  })
}

/// An allowed decision, reduced to what the middleware keeps.
struct Allowed {
  principal: Principal,
  tier: Option<String>,
  quotas: Vec<QuotaStatus>,
  leases: Vec<String>,
  signing_key: Option<String>,
}

/// Asks the platform about one action of a request.
async fn decide(
  verifier: &Verifier,
  request: &AuthorizeRequest,
  sts: &str,
) -> Result<Allowed, AuthError> {
  let AuthorizeResponse {
    decision,
    error,
    principal,
    tier,
    quotas,
    leases,
    signing_key,
  } = verifier.authorizer.authorize(request).await?;

  match (decision, principal) {
    (Decision::Allow, Some(principal)) => Ok(Allowed {
      principal,
      tier,
      quotas,
      leases,
      signing_key,
    }),
    (Decision::Allow, None) => Err(AuthError::ServiceUnavailable(
      "Authorization response is missing the principal".into(),
    )),
    (Decision::Deny, _) => {
      let (code, message) = error
        .map(|e| (e.code, e.message))
        .unwrap_or_else(|| ("AccessDenied".into(), "Access denied".into()));
      Err(match AuthError::from_code(&code, message) {
        AuthError::SignatureDoesNotMatch { message, .. } => AuthError::SignatureDoesNotMatch {
          message,
          string_to_sign: Some(sts.to_string()),
        },
        mut other => {
          if let Some(quota) = other.quota_mut() {
            *quota = quotas.into_iter().next();
          }
          other
        }
      })
    }
  }
}

/// HTTP/2 carries the host in the `:authority` pseudo-header, which servers
/// expose as the URI authority without a `Host` header.
pub fn headers_with_host<'a>(
  headers: &'a HeaderMap,
  uri: &Uri,
) -> Result<Cow<'a, HeaderMap>, AuthError> {
  if headers.contains_key(header::HOST) {
    return Ok(Cow::Borrowed(headers));
  }
  let Some(authority) = uri.authority() else {
    return Ok(Cow::Borrowed(headers));
  };
  let host = HeaderValue::from_str(authority.as_str())
    .map_err(|_| AuthError::IncompleteSignature("Request authority is not a valid host".into()))?;
  let mut owned = headers.clone();
  owned.insert(header::HOST, host);
  Ok(Cow::Owned(owned))
}

fn global_context(verifier: &Verifier, parts: &Parts) -> Vec<(&'static str, Value)> {
  request_context(parts, verifier.trust_forwarded_headers)
}

/// The condition keys every request carries: source IP, transport security
/// and user agent. `trust_forwarded_headers` takes them from
/// `x-forwarded-*`, as `Verifier::trust_forwarded_headers` describes.
pub fn request_context(parts: &Parts, trust_forwarded_headers: bool) -> Vec<(&'static str, Value)> {
  let mut context = Vec::new();
  let forwarded_ip = trust_forwarded_headers
    .then(|| header_str(parts, "x-forwarded-for"))
    .flatten()
    .and_then(|v| v.rsplit(',').next())
    .map(|ip| ip.trim().to_string());
  let source_ip = forwarded_ip.or_else(|| {
    parts
      .extensions
      .get::<ConnectInfo<SocketAddr>>()
      .map(|info| info.0.ip().to_string())
  });
  if let Some(ip) = source_ip {
    context.push(("vgr:SourceIp", Value::String(ip)));
  }
  let secure = trust_forwarded_headers && header_str(parts, "x-forwarded-proto") == Some("https");
  context.push(("vgr:SecureTransport", Value::Bool(secure)));
  if let Some(agent) = header_str(parts, header::USER_AGENT.as_str()) {
    context.push(("vgr:UserAgent", Value::String(agent.to_string())));
  }
  context
}

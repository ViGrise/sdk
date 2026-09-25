use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{ConnectInfo, OriginalUri, Request, State};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use http::header;
use http::request::Parts;
use http_body_util::{BodyExt, LengthLimitError, Limited};
use serde_json::{json, Value};

use crate::authorization::AuthorizationHeader;
use crate::canonical::{canonical_request, hex_sha256};
use crate::error::AuthError;
use crate::signer::{X_VGR_CONTENT_SHA256, X_VGR_DATE};
use crate::signing::string_to_sign;
use crate::verify::client::Authorizer;
use crate::verify::resolver::ActionResolver;
use crate::verify::types::{AuthorizeRequest, Authorized, Decision};

pub struct Verifier {
  /// Catalog namespace this service owns, for example `tts`.
  pub service: String,
  pub authorizer: Arc<dyn Authorizer>,
  pub resolver: Arc<dyn ActionResolver>,
  pub max_body_bytes: usize,
  /// Take client IP and scheme from `x-forwarded-for` / `x-forwarded-proto`.
  /// Enable only behind a proxy that overwrites these headers.
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
    (self.status(), axum::Json(json!({ "error": error }))).into_response()
  }
}

/// axum middleware: `.layer(axum::middleware::from_fn_with_state(verifier, verify))`.
pub async fn verify(
  State(verifier): State<Arc<Verifier>>,
  request: Request,
  next: Next,
) -> Response {
  let (mut parts, body) = request.into_parts();
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

  match authorize(&verifier, &parts, &body).await {
    Ok(authorized) => {
      parts.extensions.insert(authorized);
      next.run(Request::from_parts(parts, Body::from(body))).await
    }
    Err(error) => error.into_response(),
  }
}

fn header_str<'a>(parts: &'a Parts, name: &str) -> Option<&'a str> {
  parts.headers.get(name).and_then(|v| v.to_str().ok())
}

async fn authorize(
  verifier: &Verifier,
  parts: &Parts,
  body: &Bytes,
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
  let payload_hash = hex_sha256(body);
  if header_str(parts, X_VGR_CONTENT_SHA256) != Some(payload_hash.as_str()) {
    return Err(AuthError::SignatureDoesNotMatch {
      message: "x-vgr-content-sha256 does not match the request body".into(),
      string_to_sign: None,
    });
  }

  // Nested axum routers strip their prefix from `parts.uri`; the client signed the full path.
  let uri = parts
    .extensions
    .get::<OriginalUri>()
    .map(|original| original.0.clone())
    .unwrap_or_else(|| parts.uri.clone());
  let canonical = canonical_request(
    parts.method.as_str(),
    uri.path(),
    uri.query().unwrap_or(""),
    &parts.headers,
    &auth.signed_headers,
    &payload_hash,
  )?;
  let sts = string_to_sign(timestamp, &auth.scope(), &canonical);

  let resolved = verifier.resolver.resolve(parts, body)?;
  let mut context = resolved.context;
  for (key, value) in global_context(verifier, parts) {
    context.insert(key.to_string(), value);
  }

  let request = AuthorizeRequest {
    access_key_id: auth.access_key_id,
    string_to_sign: sts.clone(),
    signature: auth.signature,
    action: resolved.action,
    resources: resolved.resources,
    context,
  };
  let response = verifier.authorizer.authorize(&request).await?;

  match (response.decision, response.principal) {
    (Decision::Allow, Some(principal)) => Ok(Authorized {
      principal,
      project_plan: response.project_plan,
    }),
    (Decision::Allow, None) => Err(AuthError::ServiceUnavailable(
      "Authorization response is missing the principal".into(),
    )),
    (Decision::Deny, _) => {
      let (code, message) = response
        .error
        .map(|e| (e.code, e.message))
        .unwrap_or_else(|| ("AccessDenied".into(), "Access denied".into()));
      Err(match AuthError::from_code(&code, message) {
        AuthError::SignatureDoesNotMatch { message, .. } => AuthError::SignatureDoesNotMatch {
          message,
          string_to_sign: Some(sts),
        },
        other => other,
      })
    }
  }
}

fn global_context(verifier: &Verifier, parts: &Parts) -> Vec<(&'static str, Value)> {
  let mut context = Vec::new();
  let forwarded_ip = verifier
    .trust_forwarded_headers
    .then(|| header_str(parts, "x-forwarded-for"))
    .flatten()
    .and_then(|v| v.split(',').next())
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
  let secure =
    verifier.trust_forwarded_headers && header_str(parts, "x-forwarded-proto") == Some("https");
  context.push(("vgr:SecureTransport", Value::Bool(secure)));
  if let Some(agent) = header_str(parts, header::USER_AGENT.as_str()) {
    context.push(("vgr:UserAgent", Value::String(agent.to_string())));
  }
  context
}

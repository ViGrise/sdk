//! Sending of VGR4-signed requests, shared by the service clients.

use std::sync::Arc;

use bytes::Bytes;
use chrono::Utc;
use http::{HeaderMap, HeaderValue, Method};

use crate::auth::{
  CredentialsProvider, UNSIGNED_PAYLOAD, sign_request, sign_request_with_payload_hash,
};

/// The body of a request.
// Each service uses part of this shared code.
#[cfg_attr(not(all(feature = "speech", feature = "storage")), allow(dead_code))]
pub(crate) enum Payload {
  /// Signed with its SHA-256.
  Buffered(Bytes),
  /// Streamed and signed as `UNSIGNED-PAYLOAD`; `u64` is its length.
  Streamed(reqwest::Body, u64),
}

/// Why a signed request did not produce a successful response.
pub(crate) enum SendError {
  /// The URL could not be formed.
  InvalidUrl(String),
  /// The credentials could not be obtained or the request could not be signed.
  Signing(String),
  /// The request could not be sent or the answer could not be read.
  Transport(reqwest::Error),
  /// The service answered with an error status.
  Refused { status: u16, body: Bytes },
}

/// A client of one service endpoint. Clones share the credentials provider.
#[derive(Clone)]
pub(crate) struct SignedHttp {
  service: &'static str,
  endpoint: String,
  credentials: Arc<dyn CredentialsProvider>,
  http: reqwest::Client,
}

#[cfg_attr(not(all(feature = "speech", feature = "storage")), allow(dead_code))]
impl SignedHttp {
  pub(crate) fn new(
    service: &'static str,
    endpoint: impl AsRef<str>,
    credentials: Arc<dyn CredentialsProvider>,
  ) -> Self {
    Self {
      service,
      endpoint: endpoint.as_ref().trim_end_matches('/').to_string(),
      credentials,
      http: reqwest::Client::new(),
    }
  }

  pub(crate) fn with_endpoint(mut self, endpoint: impl AsRef<str>) -> Self {
    self.endpoint = endpoint.as_ref().trim_end_matches('/').to_string();
    self
  }

  /// The HTTP client, for requests that carry no signature.
  pub(crate) fn http(&self) -> &reqwest::Client {
    &self.http
  }

  /// Signs and sends a request to `path` of the endpoint. A refused
  /// credentials provider is told to replace its credentials.
  pub(crate) async fn send(
    &self,
    method: Method,
    path: &str,
    mut headers: HeaderMap,
    payload: Payload,
  ) -> Result<reqwest::Response, SendError> {
    let url = format!("{}{path}", self.endpoint);
    let uri: http::Uri = url
      .parse()
      .map_err(|_| SendError::InvalidUrl(format!("{url} is not a valid URL")))?;
    let credentials = self
      .credentials
      .credentials()
      .await
      .map_err(|e| SendError::Signing(e.to_string()))?;
    let (signed, body, length) = match payload {
      Payload::Buffered(bytes) => (
        sign_request(
          &method,
          &uri,
          &headers,
          &bytes,
          &credentials,
          self.service,
          Utc::now(),
        ),
        reqwest::Body::from(bytes.clone()),
        bytes.len() as u64,
      ),
      Payload::Streamed(body, length) => (
        sign_request_with_payload_hash(
          &method,
          &uri,
          &headers,
          UNSIGNED_PAYLOAD,
          &credentials,
          self.service,
          Utc::now(),
        ),
        body,
        length,
      ),
    };
    signed
      .map_err(|e| SendError::Signing(e.to_string()))?
      .apply(&mut headers)
      .map_err(|e| SendError::Signing(e.to_string()))?;
    headers.insert(http::header::CONTENT_LENGTH, HeaderValue::from(length));
    let response = self
      .http
      .request(method, url)
      .headers(headers)
      .body(body)
      .send()
      .await
      .map_err(SendError::Transport)?;
    if response.status().is_success() {
      return Ok(response);
    }
    let status = response.status().as_u16();
    let body = response.bytes().await.unwrap_or_default();
    if status == 401 || status == 403 {
      self.credentials.invalidate();
    }
    Err(SendError::Refused { status, body })
  }
}

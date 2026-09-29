use std::path::PathBuf;

use chrono::{DateTime, NaiveDateTime, Utc};
use http::{HeaderMap, HeaderName, HeaderValue, Method, Uri};
use serde::Deserialize;
use vigrise_auth::canonical::{canonical_request, hex_sha256};
use vigrise_auth::signing::{
  derive_signing_key, scope, signature, string_to_sign, TIMESTAMP_FORMAT,
};
use vigrise_auth::{sign_request, Credentials};

#[derive(Deserialize)]
struct Vector {
  request: Request,
  credentials: VectorCredentials,
  service: String,
  time: String,
  expected: Expected,
}

#[derive(Deserialize)]
struct Request {
  method: String,
  path: String,
  query: String,
  headers: Vec<(String, String)>,
  body: String,
  #[serde(default)]
  payload_hash: Option<String>,
}

#[derive(Deserialize)]
struct VectorCredentials {
  access_key_id: String,
  secret_access_key: String,
  #[serde(default)]
  session_token: Option<String>,
}

#[derive(Deserialize)]
struct Expected {
  payload_hash: String,
  signed_headers: String,
  canonical_request: String,
  string_to_sign: String,
  signature: String,
  authorization: String,
}

fn vectors() -> Vec<(String, Vector)> {
  let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-vectors/vgr4");
  let mut out: Vec<(String, Vector)> = std::fs::read_dir(&dir)
    .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
    .map(|entry| {
      let path = entry.unwrap().path();
      let vector = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
      (
        path.file_name().unwrap().to_string_lossy().to_string(),
        vector,
      )
    })
    .collect();
  out.sort_by(|a, b| a.0.cmp(&b.0));
  assert!(!out.is_empty(), "no vectors found in {}", dir.display());
  out
}

fn header_map(pairs: &[(String, String)]) -> HeaderMap {
  let mut headers = HeaderMap::new();
  for (name, value) in pairs {
    headers.append(
      HeaderName::from_bytes(name.to_ascii_lowercase().as_bytes()).unwrap(),
      HeaderValue::from_str(value).unwrap(),
    );
  }
  headers
}

#[test]
fn primitives_match_every_vector() {
  for (name, v) in vectors() {
    let payload_hash = v
      .request
      .payload_hash
      .clone()
      .unwrap_or_else(|| hex_sha256(v.request.body.as_bytes()));
    assert_eq!(
      payload_hash, v.expected.payload_hash,
      "{name}: payload hash"
    );

    let mut pairs = v.request.headers.clone();
    pairs.push(("x-vgr-date".into(), v.time.clone()));
    pairs.push(("x-vgr-content-sha256".into(), payload_hash.clone()));
    if let Some(token) = &v.credentials.session_token {
      pairs.push(("x-vgr-security-token".into(), token.clone()));
    }
    let signed: Vec<String> = v
      .expected
      .signed_headers
      .split(';')
      .map(str::to_string)
      .collect();

    let creq = canonical_request(
      &v.request.method,
      &v.request.path,
      &v.request.query,
      &header_map(&pairs),
      &signed,
      &payload_hash,
    )
    .unwrap();
    assert_eq!(
      creq, v.expected.canonical_request,
      "{name}: canonical request"
    );

    let sts = string_to_sign(&v.time, &scope(&v.time[..8], &v.service), &creq);
    assert_eq!(sts, v.expected.string_to_sign, "{name}: string to sign");

    let key = derive_signing_key(&v.credentials.secret_access_key, &v.time[..8], &v.service);
    assert_eq!(
      signature(&key, &sts),
      v.expected.signature,
      "{name}: signature"
    );
  }
}

#[test]
fn sign_request_matches_every_vector() {
  for (name, v) in vectors() {
    let host = v
      .request
      .headers
      .iter()
      .find(|(n, _)| n.eq_ignore_ascii_case("host"))
      .map(|(_, value)| value.clone())
      .unwrap();
    let query = if v.request.query.is_empty() {
      String::new()
    } else {
      format!("?{}", v.request.query)
    };
    let uri: Uri = format!("https://{host}{}{query}", v.request.path)
      .parse()
      .unwrap();
    let time: DateTime<Utc> = NaiveDateTime::parse_from_str(&v.time, TIMESTAMP_FORMAT)
      .unwrap()
      .and_utc();
    let credentials = Credentials {
      access_key_id: v.credentials.access_key_id.clone(),
      secret_access_key: v.credentials.secret_access_key.clone(),
      session_token: v.credentials.session_token.clone(),
    };
    let method = Method::from_bytes(v.request.method.as_bytes()).unwrap();
    let headers = header_map(&v.request.headers);
    let signed = match &v.request.payload_hash {
      Some(hash) => vigrise_auth::sign_request_with_payload_hash(
        &method,
        &uri,
        &headers,
        hash,
        &credentials,
        &v.service,
        time,
      ),
      None => sign_request(
        &method,
        &uri,
        &headers,
        v.request.body.as_bytes(),
        &credentials,
        &v.service,
        time,
      ),
    }
    .unwrap();
    assert_eq!(
      signed.authorization, v.expected.authorization,
      "{name}: authorization"
    );
  }
}

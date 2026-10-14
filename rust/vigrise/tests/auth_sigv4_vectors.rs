use std::path::PathBuf;

use http::{HeaderMap, HeaderName, HeaderValue};
use serde::Deserialize;
use vigrise::auth::canonical::hex_sha256;
use vigrise::auth::sigv4::{
  self, chunk_signature, derive_signing_key, string_to_sign, trailer_signature,
  HeaderAuthorization, Presigned, Scope,
};

#[derive(Deserialize)]
struct Vector {
  description: String,
  credentials: Credentials,
  region: String,
  service: String,
  time: String,
  #[serde(default)]
  presigned: bool,
  request: Request,
  signed_headers: Vec<String>,
  #[serde(default)]
  chunks: Vec<Chunk>,
  #[serde(default)]
  trailer: Vec<(String, String)>,
  expected: Expected,
}

#[derive(Deserialize)]
struct Credentials {
  access_key_id: String,
  secret_access_key: String,
}

#[derive(Deserialize)]
struct Request {
  method: String,
  path: String,
  query: String,
  headers: Vec<(String, String)>,
  payload_hash: String,
}

#[derive(Deserialize)]
struct Chunk {
  byte: String,
  length: usize,
}

#[derive(Deserialize)]
struct Expected {
  canonical_request: String,
  string_to_sign: String,
  signing_key: String,
  signature: String,
  #[serde(default)]
  authorization: Option<String>,
  #[serde(default)]
  chunk_signatures: Vec<String>,
  #[serde(default)]
  trailer_signature: Option<String>,
}

fn vectors() -> Vec<(String, Vector)> {
  let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-vectors/sigv4");
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
fn canonical_request_and_signature_match_every_vector() {
  for (name, v) in vectors() {
    let query = if v.presigned {
      sigv4::query_without_signature(&v.request.query)
    } else {
      v.request.query.clone()
    };
    let creq = sigv4::canonical_request(
      &v.request.method,
      &v.request.path,
      &query,
      &header_map(&v.request.headers),
      &v.signed_headers,
      &v.request.payload_hash,
    )
    .unwrap();
    assert_eq!(
      creq, v.expected.canonical_request,
      "{name}: canonical request"
    );

    let date = &v.time[..8];
    let scope = Scope::new(date, &v.region, &v.service);
    let sts = string_to_sign(&v.time, &scope, &creq);
    assert_eq!(
      sts, v.expected.string_to_sign,
      "{name}: string to sign ({})",
      v.description
    );

    let key = derive_signing_key(
      &v.credentials.secret_access_key,
      date,
      &v.region,
      &v.service,
    );
    assert_eq!(
      hex::encode(key),
      v.expected.signing_key,
      "{name}: signing key"
    );
    assert_eq!(
      vigrise::auth::signing::signature(&key, &sts),
      v.expected.signature,
      "{name}: signature"
    );

    if let Some(authorization) = &v.expected.authorization {
      let header = HeaderAuthorization {
        access_key_id: v.credentials.access_key_id.clone(),
        scope,
        signed_headers: v.signed_headers.clone(),
        signature: v.expected.signature.clone(),
      };
      assert_eq!(&header.to_string(), authorization, "{name}: authorization");
      assert_eq!(&HeaderAuthorization::parse(authorization).unwrap(), &header);
    }
  }
}

#[test]
fn chunk_and_trailer_signatures_match_every_vector() {
  for (name, v) in vectors().into_iter().filter(|(_, v)| !v.chunks.is_empty()) {
    let date = &v.time[..8];
    let scope = Scope::new(date, &v.region, &v.service);
    let key = derive_signing_key(
      &v.credentials.secret_access_key,
      date,
      &v.region,
      &v.service,
    );
    let mut previous = v.expected.signature.clone();
    let mut chunks: Vec<Vec<u8>> = v
      .chunks
      .iter()
      .map(|chunk| chunk.byte.as_bytes().repeat(chunk.length))
      .collect();
    chunks.push(Vec::new());
    assert_eq!(chunks.len(), v.expected.chunk_signatures.len(), "{name}");
    for (data, expected) in chunks.iter().zip(&v.expected.chunk_signatures) {
      previous = chunk_signature(&key, &v.time, &scope, &previous, &hex_sha256(data));
      assert_eq!(&previous, expected, "{name}: chunk signature");
    }
    if let Some(expected) = &v.expected.trailer_signature {
      let lines: String = v
        .trailer
        .iter()
        .map(|(name, value)| format!("{name}:{value}\n"))
        .collect();
      assert_eq!(
        &trailer_signature(&key, &v.time, &scope, &previous, lines.as_bytes()),
        expected,
        "{name}: trailer signature"
      );
    }
  }
}

#[test]
fn presigned_query_of_the_documentation_example_parses() {
  let (_, v) = vectors()
    .into_iter()
    .find(|(name, _)| name == "presigned-get.json")
    .unwrap();
  let presigned = Presigned::parse(&v.request.query).unwrap().unwrap();
  assert_eq!(presigned.access_key_id, v.credentials.access_key_id);
  assert_eq!(presigned.scope, Scope::new("20130524", "us-east-1", "s3"));
  assert_eq!(presigned.timestamp, "20130524T000000Z");
  assert_eq!(presigned.expires, 86_400);
  assert_eq!(presigned.signed_headers, vec!["host".to_string()]);
  assert_eq!(presigned.signature, v.expected.signature);
  assert_eq!(presigned.security_token, None);
}

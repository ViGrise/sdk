//! Regenerates the shared VGR4 vectors with the Rust signer and canonicalization.
use std::{error::Error, path::PathBuf};

use chrono::NaiveDateTime;
use http::{HeaderMap, HeaderName, HeaderValue, Method, Uri};
use serde_json::{Value, json};
use vigrise::auth::{
  Credentials,
  canonical::{canonical_request, hex_sha256},
  sign_request_with_payload_hash,
  signing::{TIMESTAMP_FORMAT, scope, string_to_sign},
};

fn main() -> Result<(), Box<dyn Error>> {
  let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-vectors");
  for entry in std::fs::read_dir(root.join("inputs"))? {
    let path = entry?.path();
    if path.extension().and_then(|s| s.to_str()) != Some("json") {
      continue;
    }
    let mut vector: Value = serde_json::from_str(&std::fs::read_to_string(&path)?)?;
    let request = &vector["request"];
    let mut headers = HeaderMap::new();
    for pair in request["headers"].as_array().ok_or("headers missing")? {
      headers.append(
        HeaderName::from_bytes(pair[0].as_str().ok_or("header name missing")?.as_bytes())?,
        HeaderValue::from_str(pair[1].as_str().ok_or("header value missing")?)?,
      );
    }
    let text = |value: &Value| value.as_str().map(str::to_owned).ok_or("string missing");
    let method: Method = text(&request["method"])?.parse()?;
    let path_value = text(&request["path"])?;
    let query = text(&request["query"])?;
    let uri: Uri = format!(
      "https://{}{}{}",
      headers["host"].to_str()?,
      path_value,
      if query.is_empty() {
        String::new()
      } else {
        format!("?{query}")
      }
    )
    .parse()?;
    let payload_hash = request["payload_hash"]
      .as_str()
      .map(str::to_owned)
      .unwrap_or_else(|| hex_sha256(request["body"].as_str().unwrap_or("").as_bytes()));
    let credentials = Credentials {
      access_key_id: text(&vector["credentials"]["access_key_id"])?,
      secret_access_key: text(&vector["credentials"]["secret_access_key"])?,
      session_token: vector["credentials"]["session_token"]
        .as_str()
        .map(str::to_owned),
    };
    let time = text(&vector["time"])?;
    let service = text(&vector["service"])?;
    let signed = sign_request_with_payload_hash(
      &method,
      &uri,
      &headers,
      &payload_hash,
      &credentials,
      &service,
      NaiveDateTime::parse_from_str(&time, TIMESTAMP_FORMAT)?.and_utc(),
    )?;
    signed.apply(&mut headers)?;
    let signed_names = signed
      .authorization
      .split("SignedHeaders=")
      .nth(1)
      .ok_or("signed headers missing")?
      .split(',')
      .next()
      .ok_or("signed headers missing")?;
    let names: Vec<String> = signed_names.split(';').map(str::to_owned).collect();
    let canonical = canonical_request(
      method.as_str(),
      &path_value,
      &query,
      &headers,
      &names,
      &payload_hash,
    )?;
    let sts = string_to_sign(&time, &scope(&time[..8], &service), &canonical);
    let signature = signed
      .authorization
      .split("Signature=")
      .nth(1)
      .ok_or("signature missing")?;
    vector["expected"] = json!({"payload_hash": payload_hash, "signed_headers": signed_names, "canonical_request": canonical, "string_to_sign": sts, "signature": signature, "authorization": signed.authorization});
    std::fs::write(
      root
        .join("vgr4")
        .join(path.file_name().ok_or("filename missing")?),
      format!("{}\n", serde_json::to_string_pretty(&vector)?),
    )?;
  }
  Ok(())
}

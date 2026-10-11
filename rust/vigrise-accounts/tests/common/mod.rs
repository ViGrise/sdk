#![allow(dead_code)]

use std::net::SocketAddr;

use jsonwebtoken::{jwk::JwkSet, EncodingKey, Header};
use serde_json::Value;

pub const KID: &str = "test-key";

pub fn jwks() -> JwkSet {
    serde_json::from_str(include_str!("../fixtures/jwks.json")).unwrap()
}

pub fn sign(claims: &Value) -> String {
    let key =
        EncodingKey::from_rsa_pem(include_bytes!("../fixtures/test-signing-key.pem")).unwrap();
    let mut header = Header::new(jsonwebtoken::Algorithm::RS256);
    header.kid = Some(KID.to_string());
    jsonwebtoken::encode(&header, claims, &key).unwrap()
}

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

pub async fn serve(app: axum::Router) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    addr
}

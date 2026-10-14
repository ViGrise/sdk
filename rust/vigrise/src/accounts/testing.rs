//! Helpers for testing back-channel logout and ID token verification in apps
//! with a real signing key.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{EncodingKey, Header, jwk::JwkSet};
use rsa::{
  RsaPrivateKey,
  pkcs8::{EncodePrivateKey, LineEnding},
  traits::PublicKeyParts,
};

use crate::accounts::Jwks;

const BACKCHANNEL_EVENT: &str = "http://schemas.openid.net/event/backchannel-logout";

/// A freshly generated RSA key that signs tokens the way Accounts does.
pub struct TestSigner {
  kid: String,
  encoding_key: EncodingKey,
  jwk: serde_json::Value,
}

impl Default for TestSigner {
  fn default() -> Self {
    Self::generate()
  }
}

impl TestSigner {
  /// Generates a 2048 bit RSA key with a random key id.
  pub fn generate() -> Self {
    let private = RsaPrivateKey::new(&mut rand_core::OsRng, 2048).expect("generate RSA key");
    let pem = private
      .to_pkcs8_pem(LineEnding::LF)
      .expect("encode RSA key");
    let kid = crate::accounts::oauth2::random_state();
    let jwk = serde_json::json!({
        "kty": "RSA",
        "use": "sig",
        "alg": "RS256",
        "kid": kid,
        "n": URL_SAFE_NO_PAD.encode(private.n().to_bytes_be()),
        "e": URL_SAFE_NO_PAD.encode(private.e().to_bytes_be()),
    });
    Self {
      encoding_key: EncodingKey::from_rsa_pem(pem.as_bytes()).expect("load RSA key"),
      kid,
      jwk,
    }
  }

  /// The public key as a fixed [`Jwks`].
  pub fn jwks(&self) -> Jwks {
    Jwks::from_set(self.jwk_set())
  }

  /// The public key as a JWK set, for serving from a fake JWKS endpoint.
  pub fn jwk_set(&self) -> JwkSet {
    serde_json::from_value(serde_json::json!({ "keys": [self.jwk] })).expect("build JWK set")
  }

  /// Signs arbitrary claims as an RS256 JWT with this key's id.
  pub fn sign(&self, claims: &serde_json::Value) -> String {
    let mut header = Header::new(jsonwebtoken::Algorithm::RS256);
    header.kid = Some(self.kid.clone());
    jsonwebtoken::encode(&header, claims, &self.encoding_key).expect("sign token")
  }

  /// A valid back-channel `logout_token` for session `sid` with a fresh `jti`.
  pub fn logout_token(&self, issuer: &str, client_id: &str, sid: &str) -> String {
    let now = chrono::Utc::now().timestamp();
    self.sign(&serde_json::json!({
        "iss": issuer,
        "aud": [client_id],
        "sub": "test-subject",
        "sid": sid,
        "iat": now,
        "exp": now + 300,
        "jti": crate::accounts::oauth2::random_state(),
        "events": { BACKCHANNEL_EVENT: {} },
    }))
  }
}

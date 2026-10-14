mod common;

use serde_json::{Value, json};
use vigrise::accounts::{AccountsError, BackchannelLogout, BackchannelOutcome, Jwks};

const ISS: &str = "https://accounts.vigrise.com";

fn verifier() -> BackchannelLogout {
  BackchannelLogout::new(ISS, "vigrise-drive", Jwks::from_set(common::jwks()))
}

fn claims() -> Value {
  let now = common::now();
  json!({
      "iss": ISS, "sub": "user-1", "aud": ["vigrise-drive"], "iat": now, "exp": now + 300,
      "jti": uuid_like(), "sid": "sid-1",
      "events": {"http://schemas.openid.net/event/backchannel-logout": {}}
  })
}

fn uuid_like() -> String {
  use std::sync::atomic::{AtomicU64, Ordering};
  static N: AtomicU64 = AtomicU64::new(0);
  format!("jti-{}", N.fetch_add(1, Ordering::SeqCst))
}

async fn check(
  mutate: impl FnOnce(&mut Value),
) -> Result<vigrise::accounts::LogoutToken, AccountsError> {
  let mut value = claims();
  mutate(&mut value);
  verifier()
    .verify(&common::sign(&value))
    .await
    .map(BackchannelOutcome::into_token)
}

#[tokio::test]
async fn a_valid_logout_token_yields_sid_and_subject() {
  let token = check(|_| {}).await.unwrap();
  assert_eq!(token.sid.as_deref(), Some("sid-1"));
  assert_eq!(token.sub.as_deref(), Some("user-1"));
}

#[tokio::test]
async fn wrong_issuer_audience_or_missing_event_is_rejected() {
  assert!(
    check(|c| c["iss"] = json!("https://evil.example"))
      .await
      .is_err()
  );
  assert!(
    check(|c| c["aud"] = json!(["vigrise-wallet"]))
      .await
      .is_err()
  );
  assert!(check(|c| c["events"] = json!({})).await.is_err());
  assert!(check(|c| c["nonce"] = json!("n")).await.is_err());
  assert!(
    check(|c| {
      c.as_object_mut().unwrap().remove("sid");
      c.as_object_mut().unwrap().remove("sub");
    })
    .await
    .is_err()
  );
  assert!(
    check(|c| {
      c.as_object_mut().unwrap().remove("jti");
    })
    .await
    .is_err()
  );
}

#[tokio::test]
async fn the_time_window_is_enforced() {
  let now = common::now();
  assert!(check(|c| c["exp"] = json!(now - 600)).await.is_err());
  assert!(check(|c| c["iat"] = json!(now - 1200)).await.is_err());
  assert!(check(|c| c["iat"] = json!(now + 1200)).await.is_err());
}

#[tokio::test]
async fn a_tampered_or_unsigned_token_is_rejected() {
  let token = common::sign(&claims());
  let mut parts: Vec<&str> = token.split('.').collect();
  let other = common::sign(&json!({"iss": ISS}));
  let other_payload = other.split('.').nth(1).unwrap();
  parts[1] = other_payload;
  assert!(verifier().verify(&parts.join(".")).await.is_err());
  assert!(verifier().verify("not-a-jwt").await.is_err());
}

#[tokio::test]
async fn a_replayed_jti_is_reported_as_replayed() {
  let verifier = verifier();
  let token = common::sign(&claims());
  let first = verifier.verify(&token).await.unwrap();
  assert!(matches!(first, BackchannelOutcome::Verified(_)));
  let second = verifier.verify(&token).await.unwrap();
  assert!(matches!(second, BackchannelOutcome::Replayed(_)));
  assert_eq!(second.into_token().sid.as_deref(), Some("sid-1"));
}

#[cfg(feature = "accounts-testing")]
mod with_generated_keys {
  use super::*;
  use vigrise::accounts::testing::TestSigner;

  #[tokio::test]
  async fn a_generated_key_signs_a_token_the_verifier_accepts() {
    let signer = TestSigner::generate();
    let verifier = BackchannelLogout::new(ISS, "vigrise-drive", signer.jwks());
    let token = signer.logout_token(ISS, "vigrise-drive", "sid-9");
    let outcome = verifier.verify(&token).await.unwrap();
    assert_eq!(outcome.into_token().sid.as_deref(), Some("sid-9"));
    let other = TestSigner::generate();
    assert!(
      verifier
        .verify(&other.logout_token(ISS, "vigrise-drive", "sid-9"))
        .await
        .is_err()
    );
  }
}

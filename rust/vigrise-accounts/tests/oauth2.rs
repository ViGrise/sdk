mod common;

use axum::{routing::post, Form, Json, Router};
use serde_json::{json, Value};
use std::collections::HashMap;
use vigrise_accounts::oauth2::{
    end_session_url, random_state, AuthorizationRequest, IdTokenVerifier, OAuth2Client, Pkce,
};
use vigrise_accounts::Jwks;

const ISS: &str = "https://accounts.vigrise.com";

#[test]
fn pkce_challenge_matches_the_rfc_7636_vector() {
    let pkce = Pkce::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk");
    assert_eq!(
        pkce.challenge,
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
}

#[test]
fn generated_verifiers_and_states_are_unique_and_url_safe() {
    let (a, b) = (Pkce::generate(), Pkce::generate());
    assert_ne!(a.verifier, b.verifier);
    assert_eq!(a.verifier.len(), 43);
    assert!(a
        .verifier
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'));
    assert_ne!(random_state(), random_state());
}

#[test]
fn the_authorization_url_carries_pkce_state_and_options() {
    let request = AuthorizationRequest {
        authorization_endpoint: "https://accounts.vigrise.com/o/oauth2/auth".into(),
        client_id: "vigrise-platform-console".into(),
        redirect_uri: "https://console.vigrise.com/callback".into(),
        scope: "openid email".into(),
        state: "st".into(),
        code_challenge: "ch".into(),
        prompt: Some("select_account".into()),
        max_age: Some(43200),
        login_hint: None,
    };
    let url = url::Url::parse(&request.url().unwrap()).unwrap();
    let q: HashMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(q["response_type"], "code");
    assert_eq!(q["code_challenge"], "ch");
    assert_eq!(q["code_challenge_method"], "S256");
    assert_eq!(q["state"], "st");
    assert_eq!(q["prompt"], "select_account");
    assert_eq!(q["max_age"], "43200");
    assert_eq!(q["scope"], "openid email");
    assert!(!q.contains_key("login_hint"));
}

#[test]
fn the_end_session_url_carries_the_hint_and_redirect() {
    let url = end_session_url(
        "https://accounts.vigrise.com/o/oauth2/sessions/logout",
        "vigrise-drive",
        "https://drive.vigrise.com/",
        Some("idt"),
        Some("s"),
    )
    .unwrap();
    let q: HashMap<_, _> = url::Url::parse(&url)
        .unwrap()
        .query_pairs()
        .into_owned()
        .collect();
    assert_eq!(q["client_id"], "vigrise-drive");
    assert_eq!(q["id_token_hint"], "idt");
    assert_eq!(q["post_logout_redirect_uri"], "https://drive.vigrise.com/");
    assert_eq!(q["state"], "s");
}

#[tokio::test]
async fn the_code_exchange_posts_the_verifier_and_credentials() {
    async fn token(
        Form(form): Form<HashMap<String, String>>,
    ) -> (axum::http::StatusCode, Json<Value>) {
        if form.get("grant_type").map(String::as_str) != Some("authorization_code")
            || form.get("code_verifier").map(String::as_str) != Some("ver")
            || form.get("client_secret").map(String::as_str) != Some("sec")
            || form.get("redirect_uri").map(String::as_str)
                != Some("https://console.vigrise.com/callback")
        {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(json!({"error": "invalid_grant"})),
            );
        }
        (
            axum::http::StatusCode::OK,
            Json(
                json!({"access_token": "at", "id_token": "it", "token_type": "bearer", "expires_in": 60}),
            ),
        )
    }
    let addr = common::serve(Router::new().route("/token", post(token))).await;
    let client = OAuth2Client::new(
        format!("http://{addr}/token"),
        "vigrise-platform-console",
        "sec",
        "https://console.vigrise.com/callback",
    );
    let tokens = client.exchange_code("code", "ver").await.unwrap();
    assert_eq!(tokens.access_token, "at");
    assert_eq!(tokens.id_token.as_deref(), Some("it"));
    assert!(client.exchange_code("code", "bad").await.is_err());
}

fn id_claims() -> Value {
    let now = common::now();
    json!({"iss": ISS, "sub": "u1", "aud": ["vigrise-drive"], "iat": now, "exp": now + 300,
        "sid": "sid-1", "auth_time": now - 10, "nonce": "n1"})
}

fn id_verifier() -> IdTokenVerifier {
    IdTokenVerifier::new(ISS, "vigrise-drive", Jwks::from_set(common::jwks()))
}

#[tokio::test]
async fn id_tokens_are_verified_for_issuer_audience_and_nonce() {
    let claims = id_verifier()
        .verify(&common::sign(&id_claims()), Some("n1"))
        .await
        .unwrap();
    assert_eq!(claims.sub, "u1");
    assert_eq!(claims.sid.as_deref(), Some("sid-1"));

    assert!(id_verifier()
        .verify(&common::sign(&id_claims()), Some("other"))
        .await
        .is_err());
    let mut wrong_aud = id_claims();
    wrong_aud["aud"] = json!(["someone-else"]);
    assert!(id_verifier()
        .verify(&common::sign(&wrong_aud), None)
        .await
        .is_err());
    let mut expired = id_claims();
    expired["exp"] = json!(common::now() - 600);
    assert!(id_verifier()
        .verify(&common::sign(&expired), None)
        .await
        .is_err());
}

#[tokio::test]
async fn a_public_client_exchanges_a_code_without_a_secret() {
    async fn token(
        Form(form): Form<HashMap<String, String>>,
    ) -> (axum::http::StatusCode, Json<Value>) {
        let ok = form.get("client_id").map(String::as_str) == Some("vigrise-wallet-desktop")
            && form.get("code_verifier").map(String::as_str) == Some("ver")
            && form.get("redirect_uri").map(String::as_str) == Some("http://127.0.0.1:4455/cb")
            && !form.contains_key("client_secret");
        if ok {
            (
                axum::http::StatusCode::OK,
                Json(json!({"access_token": "at", "id_token": "it"})),
            )
        } else {
            (
                axum::http::StatusCode::BAD_REQUEST,
                Json(json!({"error": "invalid_request"})),
            )
        }
    }
    let addr = common::serve(Router::new().route("/token", post(token))).await;
    let client = OAuth2Client::public(
        format!("http://{addr}/token"),
        "vigrise-wallet-desktop",
        "http://127.0.0.1:4455/cb",
    );
    let tokens = client.exchange_code("code", "ver").await.unwrap();
    assert_eq!(tokens.id_token.as_deref(), Some("it"));
}

#[tokio::test]
async fn a_slow_token_endpoint_times_out_as_unavailable() {
    async fn slow() -> Json<Value> {
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        Json(json!({}))
    }
    let addr = common::serve(Router::new().route("/token", post(slow))).await;
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(200))
        .build()
        .unwrap();
    let client = OAuth2Client::public(format!("http://{addr}/token"), "c", "http://127.0.0.1/cb")
        .with_client(http);
    assert!(matches!(
        client.exchange_code("code", "ver").await,
        Err(vigrise_accounts::AccountsError::Unavailable(_))
    ));
}

#[tokio::test]
async fn verify_id_token_checks_the_given_audience() {
    let mut claims = id_claims();
    claims["aud"] = json!(["vigrise-wallet-desktop"]);
    let token = common::sign(&claims);
    let verifier = id_verifier();
    let verified = verifier
        .verify_id_token(&token, "vigrise-wallet-desktop")
        .await
        .unwrap();
    assert_eq!(verified.sub, "u1");
    assert!(verifier
        .verify_id_token(&token, "vigrise-drive")
        .await
        .is_err());
}

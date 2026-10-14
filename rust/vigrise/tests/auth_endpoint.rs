use std::collections::HashMap;

use vigrise::auth::endpoint::{env_key, resolve_endpoint_with, sts_base_url_with, sts_url_with};

fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
  let map: HashMap<String, String> = pairs
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
  move |name| map.get(name).cloned()
}

#[test]
fn an_explicit_endpoint_wins_over_the_environment() {
  let lookup = env(&[
    ("VIGRISE_ENDPOINT_URL_SPEECH", "http://speech.env"),
    ("VIGRISE_ENDPOINT_URL", "http://all.env"),
  ]);
  assert_eq!(
    resolve_endpoint_with("speech", Some("http://explicit/"), lookup),
    "http://explicit"
  );
}

#[test]
fn the_service_variable_wins_over_the_shared_one() {
  let lookup = env(&[
    ("VIGRISE_ENDPOINT_URL_SPEECH", "http://speech.env"),
    ("VIGRISE_ENDPOINT_URL", "http://all.env"),
  ]);
  assert_eq!(
    resolve_endpoint_with("speech", None, &lookup),
    "http://speech.env"
  );
  assert_eq!(
    resolve_endpoint_with("storage", None, &lookup),
    "http://all.env"
  );
}

#[test]
fn the_default_host_follows_the_service_name() {
  let lookup = env(&[]);
  assert_eq!(
    resolve_endpoint_with("speech", None, &lookup),
    "https://speech-api.vigrise.com"
  );
  assert_eq!(
    resolve_endpoint_with("storage", None, &lookup),
    "https://storage-api.vigrise.com"
  );
}

#[test]
fn empty_values_are_ignored() {
  let lookup = env(&[
    ("VIGRISE_ENDPOINT_URL_SPEECH", ""),
    ("VIGRISE_ENDPOINT_URL", ""),
  ]);
  assert_eq!(
    resolve_endpoint_with("speech", Some(""), lookup),
    "https://speech-api.vigrise.com"
  );
}

#[test]
fn service_names_become_upper_case_keys_with_underscores() {
  assert_eq!(env_key("speech"), "VIGRISE_ENDPOINT_URL_SPEECH");
  assert_eq!(env_key("voice-studio"), "VIGRISE_ENDPOINT_URL_VOICE_STUDIO");
}

#[test]
fn the_sts_url_ends_in_the_assume_role_path() {
  assert_eq!(
    sts_url_with(None, env(&[])),
    "https://sts-api.vigrise.com/v1/assume-role"
  );
  assert_eq!(
    sts_url_with(None, env(&[("VIGRISE_ENDPOINT_URL", "http://local:8080")])),
    "http://local:8080/v1/assume-role"
  );
  assert_eq!(
    sts_url_with(
      None,
      env(&[(
        "VIGRISE_ENDPOINT_URL_STS",
        "http://sts.internal/v1/assume-role"
      )])
    ),
    "http://sts.internal/v1/assume-role"
  );
  assert_eq!(
    sts_url_with(Some("http://explicit"), env(&[])),
    "http://explicit/v1/assume-role"
  );
}

#[test]
fn the_sts_base_url_drops_the_assume_role_path() {
  assert_eq!(
    sts_base_url_with(None, env(&[])),
    "https://sts-api.vigrise.com"
  );
  assert_eq!(
    sts_base_url_with(
      None,
      env(&[(
        "VIGRISE_ENDPOINT_URL_STS",
        "http://sts.internal/v1/assume-role"
      )])
    ),
    "http://sts.internal"
  );
}

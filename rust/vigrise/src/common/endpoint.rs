//! Where a ViGrise service lives: the explicit endpoint, then
//! `VIGRISE_ENDPOINT_URL_<SERVICE>`, then `VIGRISE_ENDPOINT_URL`, then
//! `https://<service>-api.vigrise.com`.

/// Environment variable that points every service at one endpoint.
pub const ENV_ENDPOINT_URL: &str = "VIGRISE_ENDPOINT_URL";

/// Path of the role session call of the STS endpoint.
pub const ASSUME_ROLE_PATH: &str = "/v1/assume-role";

/// The environment variable of one service, such as
/// `VIGRISE_ENDPOINT_URL_VOICE_STUDIO` for `voice-studio`.
pub fn env_key(service: &str) -> String {
  format!(
    "{ENV_ENDPOINT_URL}_{}",
    service.to_uppercase().replace('-', "_")
  )
}

/// The endpoint of `service`, reading the process environment.
pub fn resolve_endpoint(service: &str, explicit: Option<&str>) -> String {
  resolve_endpoint_with(service, explicit, |name| std::env::var(name).ok())
}

/// Like [`resolve_endpoint`], reading environment variables through `lookup`.
/// Empty values count as unset and a trailing slash is dropped.
pub fn resolve_endpoint_with(
  service: &str,
  explicit: Option<&str>,
  lookup: impl Fn(&str) -> Option<String>,
) -> String {
  let present = |value: Option<String>| value.filter(|value| !value.is_empty());
  let endpoint = present(explicit.map(str::to_string))
    .or_else(|| present(lookup(&env_key(service))))
    .or_else(|| present(lookup(ENV_ENDPOINT_URL)))
    .unwrap_or_else(|| format!("https://{service}-api.vigrise.com"));
  endpoint.trim_end_matches('/').to_string()
}

/// The URL of the role session call. The STS endpoint is accepted with or
/// without the `/v1/assume-role` path.
pub fn sts_url(explicit: Option<&str>) -> String {
  sts_url_with(explicit, |name| std::env::var(name).ok())
}

/// Like [`sts_url`], reading environment variables through `lookup`.
pub fn sts_url_with(explicit: Option<&str>, lookup: impl Fn(&str) -> Option<String>) -> String {
  let endpoint = resolve_endpoint_with("sts", explicit, lookup);
  if endpoint.ends_with(ASSUME_ROLE_PATH) {
    endpoint
  } else {
    format!("{endpoint}{ASSUME_ROLE_PATH}")
  }
}

/// The base URL of the STS endpoint, for calls other than the role session
/// call. It is the endpoint without the `/v1/assume-role` path.
pub fn sts_base_url(explicit: Option<&str>) -> String {
  sts_base_url_with(explicit, |name| std::env::var(name).ok())
}

/// Like [`sts_base_url`], reading environment variables through `lookup`.
pub fn sts_base_url_with(
  explicit: Option<&str>,
  lookup: impl Fn(&str) -> Option<String>,
) -> String {
  let endpoint = resolve_endpoint_with("sts", explicit, lookup);
  endpoint
    .strip_suffix(ASSUME_ROLE_PATH)
    .unwrap_or(&endpoint)
    .to_string()
}

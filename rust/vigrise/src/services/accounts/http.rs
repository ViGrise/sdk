use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// A `reqwest::Client` with the connect (2 s) and total (5 s) timeouts the
/// ViGrise Accounts helpers use by default.
pub(crate) fn default_http_client() -> reqwest::Client {
  reqwest::Client::builder()
    .connect_timeout(CONNECT_TIMEOUT)
    .timeout(REQUEST_TIMEOUT)
    .build()
    .unwrap_or_default()
}

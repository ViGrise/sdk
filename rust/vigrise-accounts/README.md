# vigrise-accounts

ViGrise Accounts for Rust services: SSO introspection, back-channel logout
verification and OAuth2/OIDC helpers.

- `SsoIntrospector` introspects the shared `vigrise_sso` cookie value over the
  internal network with the app's client credentials and caches answers for 30
  seconds by token hash.
- `BackchannelLogout` verifies the `logout_token` Accounts sends when a session
  ends: signature against the JWKS, `iss`, `aud`, the logout event, the time
  window and `jti` replay (a replay is reported as `BackchannelOutcome::Replayed`,
  to answer 200).
- `oauth2` has PKCE, `state`, the authorization URL, the code exchange, ID
  token verification and the end-session URL for apps on other domains.
- With the `testing` feature, `testing::TestSigner` generates an RSA key, its
  JWKS and signed `logout_token`s for app tests.
- With the `axum` feature, `AccountsIdentity` is an extractor that resolves the
  `X-ViGrise-SSO` header to the signed-in person.

## Installation

```sh
cargo add vigrise-accounts
```

## Example

```rust
use vigrise_accounts::{SsoIntrospector, SsoIntrospectorConfig};

let config = SsoIntrospectorConfig::new(
    "http://accounts-backend:8080/o/auth",
    "vigrise-drive",
    "client-secret",
);
let introspector = SsoIntrospector::new(config);

let answer = introspector.introspect("value of the vigrise_sso cookie").await?;
if answer.active {
    println!("signed in as {:?}", answer.identity.unwrap().email);
}
```

## License

Apache-2.0

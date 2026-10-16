# vigrise-sso

First-party SSO for Rust apps on `*.vigrise.com`. This crate is for ViGrise's own
apps. The app's Accounts client needs `first_party` and `sso_introspection`,
which only operators set. Apps on other domains use `vigrise::accounts::oauth2`
from the [`vigrise`](https://github.com/ViGrise/sdk/tree/main/rust/vigrise) crate instead.

```sh
cargo add vigrise-sso
```

| Feature | Adds |
| --- | --- |
| `axum` | `AccountsIdentity` extractor and `SSO_HEADER` for `axum` |

## Introspect the SSO cookie

`SsoIntrospector` posts the `vigrise_sso` cookie value to the internal Accounts
URL with the app's client credentials and caches answers by token hash for 30
seconds.

```rust
use vigrise_sso::{SsoIntrospector, SsoIntrospectorConfig};

let introspector = SsoIntrospector::new(SsoIntrospectorConfig::new(
    "http://accounts-backend:8080/o/auth", "vigrise-drive", "client-secret"));
let answer = introspector.introspect("value of the vigrise_sso cookie").await?;
if answer.active {
    println!("signed in as {:?}", answer.identity.unwrap().email);
}
```

An unknown, revoked or ended token answers `active: false`. Errors split into
unavailable (Accounts unreachable, rate limiting or failing: retry later,
counted by `unavailable_count`) and misconfigured (rejected client credentials
or an unexpected answer). `invalidate_session(sid)` and `invalidate_token(hash)`
drop cached answers; the cache lives in the process.

## axum extractor

With the `axum` feature, `AccountsIdentity` resolves the person of a request
from the `x-vigrise-sso` header through a shared `Arc<SsoIntrospector>` in the
app state (`FromRef`). It rejects with 401 `SsoSessionInactive`, 503
`AccountsUnavailable` or 500 `AccountsMisconfigured`.

## License

Apache-2.0

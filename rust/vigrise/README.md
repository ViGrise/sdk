# vigrise

The ViGrise SDK for Rust: request signing, credentials and clients for ViGrise
services in one crate, each service behind a Cargo feature. Requires Rust 1.85 (edition 2024) and,
for the clients, Tokio.

```sh
cargo add vigrise --features speech,storage
```

## Features

No feature is on by default. `vigrise::auth` is always available.

| Feature | Adds |
| --- | --- |
| `reqwest` | `auth::reqwest_signer::sign_reqwest`: signs a built `reqwest::Request` |
| `sts` | `auth::sts`: `AssumeRoleProvider` keeps a role session fresh |
| `verify` | `auth::verify`: an `axum` middleware that authorizes incoming requests (implies `sts`) |
| `sigv4` | AWS Signature Version 4 verification with aws-chunked bodies and additional checksums (implies `verify`) |
| `metering` | `auth::metering`: background reporting of usage records (implies `verify`) |
| `openapi` | `utoipa` schemas for the wire types |
| `accounts` | `vigrise::accounts`: OAuth2/OIDC helpers and back-channel logout |
| `accounts-testing` | `accounts::testing::TestSigner` for app tests |
| `storage` | `vigrise::storage`: the Storage client |
| `speech` | `vigrise::speech`: the Speech client and webhook verification |

## Guides

Each service has a guide with examples in
[`docs/`](https://github.com/ViGrise/sdk/tree/main/docs), and runnable examples live in
[`rust/vigrise/examples/`](https://github.com/ViGrise/sdk/tree/main/rust/vigrise/examples). API reference:
<https://docs.rs/vigrise>.

## License

Apache-2.0

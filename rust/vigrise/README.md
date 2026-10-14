# vigrise

The ViGrise SDK for Rust: VGR4 request signing, ViGrise Accounts, ViGrise
Storage and the Speech API in one crate. Requires Rust 1.85 (edition 2024) and,
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
| `accounts` | `vigrise::accounts`: SSO introspection, back-channel logout, OAuth2/OIDC |
| `accounts-axum` | `AccountsIdentity` extractor for `axum` |
| `accounts-testing` | `accounts::testing::TestSigner` for app tests |
| `storage` | `vigrise::storage`: the Storage client |
| `speech` | `vigrise::speech`: the Speech client and webhook verification |

## Guides

[Auth](../../docs/auth/README.md), [Speech](../../docs/speech/README.md),
[Storage](../../docs/storage/README.md) and
[Accounts](../../docs/accounts/README.md). API reference:
<https://docs.rs/vigrise>. Examples: `examples/speech.rs`,
`examples/speech_tour.rs`.

## License

Apache-2.0

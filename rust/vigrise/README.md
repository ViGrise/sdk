# vigrise

The ViGrise SDK for Rust: VGR4 request signing, ViGrise Accounts, ViGrise
Storage and the Speech API in one crate. VGR4 is AWS Signature Version 4
without the region component: the credential scope is
`<yyyymmdd>/<service>/vgr4_request`.

```sh
cargo add vigrise --features speech,storage
```

## Features

| Feature | Adds |
| --- | --- |
| (always) | `vigrise::auth`: VGR4 signing and credentials |
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

No feature is on by default.

## Credentials

`Credentials::resolve()` reads `VIGRISE_ACCESS_KEY_ID` and
`VIGRISE_SECRET_ACCESS_KEY` (with `VIGRISE_SESSION_TOKEN` for a role session),
then profile `VIGRISE_PROFILE` (default `default`) in
`VIGRISE_SHARED_CREDENTIALS_FILE` (default `~/.vigrise/credentials`). Clients
take any `CredentialsProvider`; `sts::AssumeRoleProvider` refreshes a role
session before it expires.

## Speech

```rust
use std::sync::Arc;
use vigrise::auth::Credentials;
use vigrise::speech::{JobInput, JobOptions, SpeechClient, SynthesizeOptions, WaitOptions};

let speech = SpeechClient::new(Arc::new(Credentials::resolve()?));
let spoken = speech.synthesize("Xin chào", "haley-f1", &SynthesizeOptions::default()).await?;
let job = speech
    .create_job(&JobInput::Text { text: "A long text".into(), voice: "haley-f1".into() }, &JobOptions::default())
    .await?;
let job = speech.wait_for_job(&job.id, &WaitOptions::default()).await?;
```

`with_endpoint` points the client elsewhere than `https://speech.vigrise.com`.
`synthesize_stream` returns the audio as a `Stream`. Errors are `SpeechError`:
`Service { status, code, message, resource_id, details }` for the service's
error envelope, and distinct `Transport`, `Timeout`, `Signing` and
`WaitTimeout { last_job }` variants. Nothing is retried; pass
`idempotency_key` to repeat a call safely.

Webhooks of a job are verified without credentials or a client:

```rust
let event = vigrise::speech::verify_webhook(&secret, &headers, &raw_body)?;
```

Examples: `examples/speech.rs`, `examples/speech_tour.rs`
(`cargo run --example speech_tour --features speech -- <url> <voice>`).

## Storage

```rust
use vigrise::storage::{PutOptions, StorageClient};

let storage = StorageClient::new("https://storage.vigrise.com", Arc::new(Credentials::resolve()?));
storage.create_bucket("photos").await?;
storage.put_object("photos", "2026/cat.jpg", &b"..."[..], &PutOptions::default()).await?;
```

Buckets, objects (signed, or streamed as `UNSIGNED-PAYLOAD`), multipart uploads
with `upload_stream`, tags, presigned URLs and lifecycle rules.

## Accounts

```rust
use vigrise::accounts::{SsoIntrospector, SsoIntrospectorConfig};

let introspector = SsoIntrospector::new(SsoIntrospectorConfig::new(
    "http://accounts-backend:8080/o/auth", "vigrise-drive", "client-secret"));
let answer = introspector.introspect("value of the vigrise_sso cookie").await?;
```

`BackchannelLogout` verifies the `logout_token` Accounts sends when a session
ends; `accounts::oauth2` has PKCE, the authorization URL, the code exchange and
ID token verification for apps on other domains.

## Signing

```rust
use vigrise::auth::reqwest_signer::sign_reqwest;
let client = reqwest::Client::new();
let mut request = client.get("https://speech.vigrise.com/v1/voices").build()?;
sign_reqwest(&mut request, &Credentials::resolve()?, "speech", chrono::Utc::now())?;
```

Without the `reqwest` feature, `auth::sign_request` signs any `http` request.

## Documentation

API reference: <https://docs.rs/vigrise>. Other SDKs and the shared test
vectors: <https://github.com/vigrise/sdk>.

## License

Apache-2.0

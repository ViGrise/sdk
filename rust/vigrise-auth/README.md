# vigrise-auth

Request signing for calling ViGrise services, and request verification for
services that accept them. VGR4 is AWS Signature Version 4 without the region
component: the credential scope is `<yyyymmdd>/<service>/vgr4_request`, the
signing key is derived as
`HMAC(HMAC(HMAC("VGR4" + secret, date), service), "vgr4_request")`, and every
signature covers `host`, `x-vgr-date` and `x-vgr-content-sha256`.

## Installation

```sh
cargo add vigrise-auth
```

With the features you need:

```sh
cargo add vigrise-auth --features reqwest,sts
```

## Example

```rust
use vigrise_auth::reqwest_signer::sign_reqwest;
use vigrise_auth::Credentials;

let client = reqwest::Client::new();
let mut request = client.get("https://speech.vigrise.com/v1/voices").build()?;
sign_reqwest(&mut request, &Credentials::resolve()?, "speech", chrono::Utc::now())?;
let response = client.execute(request).await?;
```

This example needs the `reqwest` feature. Without it, `sign_request` signs any
`http` request and `SignedRequestHeaders::apply` writes the headers.

## Features

| Feature | Adds |
| --- | --- |
| `reqwest` | `reqwest_signer::sign_reqwest`: signs a built `reqwest::Request` |
| `sts` | `sts`: `AssumeRoleProvider` keeps a role session fresh, `service_credentials` for ViGrise services |
| `verify` | `verify`: an `axum` middleware that authorizes incoming requests (implies `sts`) |
| `sigv4` | AWS Signature Version 4 verification with aws-chunked bodies and additional checksums (implies `verify`) |
| `metering` | `metering`: background reporting of usage records (implies `verify`) |
| `openapi` | `utoipa` schemas for the wire types |

No feature is on by default. Signing with `sign_request` needs none.

## Credentials

`Credentials::resolve()` reads, in this order:

1. `VIGRISE_ACCESS_KEY_ID` and `VIGRISE_SECRET_ACCESS_KEY`.
2. Profile `VIGRISE_PROFILE` (default `default`) in
   `VIGRISE_SHARED_CREDENTIALS_FILE` (default `~/.vigrise/credentials`):

```ini
[default]
vigrise_access_key_id = VGAK...
vigrise_secret_access_key = ...
```

`Credentials::new(access_key_id, secret)` builds them explicitly. Temporary
credentials of a role session (`VGST...`) also carry a session token:
`VIGRISE_SESSION_TOKEN` or `vigrise_session_token`. Signers send it in the
signed `x-vgr-security-token` header.

## Role sessions

With the `sts` feature, `AssumeRoleProvider` assumes a role and refreshes the
session five minutes before it expires:

```rust
use vigrise_auth::sts::{AssumeRoleProvider, StsClient};
use vigrise_auth::{Credentials, CredentialsProvider};

let provider = AssumeRoleProvider::new(
    StsClient::new("https://platform.vigrise.com/api/v1/sts/assume-role", Credentials::resolve()?),
    "vrn:vigrise:iam:<project_id>:role/<name>",
    "worker",
);
let credentials = provider.credentials().await?;
```

## Documentation

API reference: <https://docs.rs/vigrise-auth>. Other SDKs and the shared test
vectors: <https://github.com/vigrise/sdk>.

## License

Apache-2.0

# Signing, credentials and role sessions

Every ViGrise machine API accepts VGR4-signed requests. The SDK signs for you:
a client of a service (`speech`, `storage`) signs its calls, and the signing
functions below cover services without a client. Other pages: [Speech](../speech/README.md),
[Storage](../storage/README.md), [Accounts](../accounts/README.md).

## VGR4

VGR4 is AWS Signature Version 4 without the region component. The credential
scope is `<yyyymmdd>/<service>/vgr4_request`, where `<service>` names the
ViGrise service being called (`speech`, `storage`, and so on). The signing key
is derived as
`HMAC(HMAC(HMAC("VGR4" + secret, date), service), "vgr4_request")`. The
canonical request follows SigV4: method, canonical URI, sorted canonical query,
canonical headers, signed header names and the hex SHA-256 of the payload. The
string to sign is `VGR4-HMAC-SHA256`, the timestamp, the scope and the hash of
the canonical request, each on its own line. Every signature covers `host`,
`x-vgr-date` (`yyyymmddThhmmssZ`) and `x-vgr-content-sha256`; temporary
credentials also sign `x-vgr-security-token`. The result travels in
`Authorization: VGR4-HMAC-SHA256 Credential=<access key>/<scope>, SignedHeaders=<names>, Signature=<hex>`.

## Signing a request

```rust
// Cargo feature `reqwest`
let mut request = client.get("https://speech-api.vigrise.com/v1/voices").build()?;
vigrise::auth::reqwest_signer::sign_reqwest(&mut request, &Credentials::resolve()?, "speech", chrono::Utc::now())?;
client.execute(request).await?;
```

```ts
import { signedFetch } from "@vigrise/sdk/auth";
const response = await signedFetch("https://speech-api.vigrise.com/v1/voices", {}, { service: "speech" });
// Browsers: import { signedFetch } from "@vigrise/sdk/auth/browser" and pass credentials.
```

```python
import requests
from vigrise.auth.requests_auth import ViGriseAuth
requests.get("https://speech-api.vigrise.com/v1/voices", auth=ViGriseAuth("speech"))
# httpx: vigrise.auth.httpx_auth.ViGriseHttpxAuth
```

Without the `reqwest` feature, `vigrise::auth::sign_request` signs any `http` request.

## Credentials

Resolved in this order:

1. Explicit `Credentials::new(access_key_id, secret)`.
2. `VIGRISE_ACCESS_KEY_ID` and `VIGRISE_SECRET_ACCESS_KEY`.
3. Profile `VIGRISE_PROFILE` (default `default`) in
   `VIGRISE_SHARED_CREDENTIALS_FILE` (default `~/.vigrise/credentials`, or
   `%USERPROFILE%\.vigrise\credentials` on Windows):

```ini
[default]
vigrise_access_key_id = VGAK...
vigrise_secret_access_key = ...
```

Temporary credentials of a role session (`VGST...`) also carry a session token:
`VIGRISE_SESSION_TOKEN` or `vigrise_session_token`. Signers send it in the signed
`x-vgr-security-token` header.

## Endpoints

Each service lives on `https://<service>-api.vigrise.com` (`speech-api`,
`storage-api`, `sts-api`). A client resolves its endpoint in this order:

1. The `endpoint` argument.
2. `VIGRISE_ENDPOINT_URL_<SERVICE>`, for example `VIGRISE_ENDPOINT_URL_SPEECH`
   or `VIGRISE_ENDPOINT_URL_STS` (the service name in upper case, `-` as `_`).
3. `VIGRISE_ENDPOINT_URL`, which points every service at one endpoint.
4. The default host.

Empty values count as unset. The resolver is `vigrise::auth::endpoint` in Rust,
`resolveEndpoint` in `@vigrise/sdk/auth` and `vigrise.auth.endpoint` in Python.

## Role sessions

`AssumeRole` (`POST https://sts-api.vigrise.com/v1/assume-role`, signed
under the `sts` scope with a long-term key) returns a session of a role that
lasts 15 minutes up to the role's maximum. An optional session policy narrows
what the session may do.

A backend handing a web page a narrowed session:

```ts
import { assumeRole } from "@vigrise/sdk/auth";
const session = await assumeRole(
  { roleVrn, roleSessionName: `user-${userId}`, durationSeconds: 900,
    sessionPolicy: { Version: "2026-10-01", Statement: { Effect: "Allow", Action: "speech:SynthesizeSpeech", Resource: "*" } } },
  {}, // the STS endpoint comes from VIGRISE_ENDPOINT_URL_STS, or https://sts-api.vigrise.com
);
// send session.credentials to the page, which signs with @vigrise/sdk/auth/browser:
import { signedFetch } from "@vigrise/sdk/auth/browser";
await signedFetch(url, init, { service: "speech", credentials: session.credentials });
```

```python
from vigrise.auth import assume_role
session = assume_role(role_vrn, "nightly-job")
requests.get(url, auth=ViGriseAuth("speech", session.credentials))
```

Rust keeps one session and refreshes it before it expires, five minutes early
(or after 80 percent of a session shorter than 25 minutes):

```rust
let provider = AssumeRoleProvider::new(StsClient::resolved(Credentials::resolve()?), role_vrn, "worker");
let credentials = provider.credentials().await?;
```

ViGrise services use `vigrise::auth::sts::service_credentials`, which assumes
`VIGRISE_ROLE_VRN` through the STS endpoint (`VIGRISE_ENDPOINT_URL_STS`) when it is set. TypeScript and
Python callers call `assumeRole` again before `expiration`; `signedFetch` and the
auth classes also accept a function returning the current credentials.

## Signing in with your ViGrise account

`vigrise login` signs you in at Accounts and writes a role session to a
profile of `~/.vigrise/credentials`, which every SDK reads:

```bash
cargo install --path rust/vigrise-cli
vigrise login --role vrn:vigrise:iam:<project_id>:role/<name> --profile dev
VIGRISE_PROFILE=dev python python/examples/speech.py https://speech-api.vigrise.com
```

It signs in at Accounts and asks the STS endpoint (`--sts-url`, `VIGRISE_ENDPOINT_URL_STS`, `https://sts-api.vigrise.com`) for the session. It opens a browser (`--device` shows a code to enter on another device
instead). Without `--role` it lists the roles you may assume. The sign-in is
cached in `~/.vigrise/sso/<profile>.json`, so the next `vigrise login` needs
no browser until it expires. `vigrise whoami` shows the session's role and
expiry; `vigrise logout` removes both. A role lets you in when its trust
policy names you as a project member, for example
`"Principal": {"VGR": "vrn:vigrise:iam:<project_id>:member/*"}`.

## Services behind a proxy

The signature covers the `Host` header. A reverse proxy in front of a VGR4
service must forward the client's original `Host` (for nginx,
`proxy_set_header Host $host;`). A rewritten host fails with
`SignatureDoesNotMatch`, and the error body carries the `string_to_sign` the
service computed for debugging.

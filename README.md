# ViGrise SDK

Request signing (VGR4) for calling ViGrise services, and verification for
services that accept VGR4 requests.

| Path | Contents |
| --- | --- |
| `test-vectors/` | Reference implementation (`generate.py`) and shared VGR4 vectors |
| `rust/vigrise-auth` | Rust signer (`reqwest` feature), role sessions (`sts` feature: `AssumeRoleProvider`) and service verifier (`verify` feature) |
| `typescript` | `@vigrise/auth`: `signRequest`, `signedFetch`, `resolveCredentials`, `assumeRole` (Node 18+); `@vigrise/auth/browser`: Web Crypto `signRequest` and `signedFetch` for browsers |
| `python` | `vigrise-auth`: `sign_request`, `assume_role`, `ViGriseAuth` for requests, `ViGriseHttpxAuth` for httpx (Python 3.9+) |
| `rust/vigrise-cli` | `vigrise` command line: `login`, `logout`, `whoami` |
| `rust/vigrise-storage` | Client for ViGrise Storage: buckets, objects (signed or streamed with `UNSIGNED-PAYLOAD`), multipart uploads with a streaming helper, tags, presigned URLs and lifecycle rules |

## VGR4 in one paragraph

VGR4 is AWS Signature Version 4 without the region component. The credential
scope is `<yyyymmdd>/<service>/vgr4_request`, the signing key is derived as
`HMAC(HMAC(HMAC("VGR4" + secret, date), service), "vgr4_request")`, and every
signature covers `host`, `x-vgr-date` and `x-vgr-content-sha256`. The full
specification lives in the platform repo:
`docs/superpowers/specs/2026-09-25-catalog-iam-1a-design.md`, section 7.

## Test vectors

`test-vectors/inputs/*.json` hold requests; `python3 test-vectors/generate.py`
writes `test-vectors/vgr4/*.json` with expected canonical request, string to
sign, signature and Authorization header. Every SDK must pass every vector. CI
fails when the committed vectors differ from a fresh generation.

## Usage

```ts
import { signedFetch } from "@vigrise/auth";
const res = await signedFetch("https://tts.vigrise.com/v1/voices", {}, { service: "tts" });
```

```python
import requests
from vigrise_auth.requests_auth import ViGriseAuth
requests.get("https://tts.vigrise.com/v1/voices", auth=ViGriseAuth("tts"))
```

```rust
let mut request = client.get("https://tts.vigrise.com/v1/voices").build()?;
vigrise_auth::reqwest_signer::sign_reqwest(&mut request, &Credentials::resolve()?, "tts", chrono::Utc::now())?;
client.execute(request).await?;
```

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

## Role sessions

`AssumeRole` (`POST https://platform.vigrise.com/api/v1/sts/assume-role`, signed
under the `sts` scope with a long-term key) returns a session of a role that
lasts 15 minutes up to the role's maximum. An optional session policy narrows
what the session may do.

A backend handing a web page a narrowed session:

```ts
import { assumeRole } from "@vigrise/auth";
const session = await assumeRole(
  { roleVrn, roleSessionName: `user-${userId}`, durationSeconds: 900,
    sessionPolicy: { Version: "2026-10-01", Statement: { Effect: "Allow", Action: "tts:SynthesizeSpeech", Resource: "*" } } },
  { endpoint: "https://platform.vigrise.com/api/v1/sts/assume-role" },
);
// send session.credentials to the page, which signs with @vigrise/auth/browser:
import { signedFetch } from "@vigrise/auth/browser";
await signedFetch(url, init, { service: "tts", credentials: session.credentials });
```

```python
from vigrise_auth import assume_role
session = assume_role(role_vrn, "nightly-job", endpoint="https://platform.vigrise.com/api/v1/sts/assume-role")
requests.get(url, auth=ViGriseAuth("tts", session.credentials))
```

Rust keeps one session and refreshes it before it expires, five minutes early
(or after 80 percent of a session shorter than 25 minutes):

```rust
let provider = AssumeRoleProvider::new(StsClient::new(sts_url, Credentials::resolve()?), role_vrn, "worker");
let credentials = provider.credentials().await?;
```

ViGrise services use `vigrise_auth::sts::service_credentials`, which assumes
`VIGRISE_ROLE_VRN` through `VIGRISE_STS_URL` when it is set. TypeScript and
Python callers call `assumeRole` again before `expiration`; `signedFetch` and the
auth classes also accept a function returning the current credentials.

## Signing in with your ViGrise account

`vigrise login` signs you in at Accounts and writes a role session to a
profile of `~/.vigrise/credentials`, which every SDK reads:

```bash
cargo install --path rust/vigrise-cli
vigrise login --role vrn:vigrise:iam:<project_id>:role/<name> --profile dev
VIGRISE_PROFILE=dev python examples/tts.py https://tts.vigrise.com
```

It opens a browser (`--device` shows a code to enter on another device
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

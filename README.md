# Vigrise SDK

Request signing (VGR4) for calling Vigrise services, and verification for
services that accept VGR4 requests.

| Path | Contents |
| --- | --- |
| `test-vectors/` | Reference implementation (`generate.py`) and shared VGR4 vectors |
| `rust/vigrise-auth` | Rust signer (`reqwest` feature) and service verifier (`verify` feature) |
| `typescript` | `@vigrise/auth`: `signRequest`, `signedFetch`, `resolveCredentials` (Node 18+) |
| `python` | `vigrise-auth`: `sign_request`, `VigriseAuth` for requests, `VigriseHttpxAuth` for httpx (Python 3.9+) |

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
from vigrise_auth.requests_auth import VigriseAuth
requests.get("https://tts.vigrise.com/v1/voices", auth=VigriseAuth("tts"))
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

## Services behind a proxy

The signature covers the `Host` header. A reverse proxy in front of a VGR4
service must forward the client's original `Host` (for nginx,
`proxy_set_header Host $host;`). A rewritten host fails with
`SignatureDoesNotMatch`, and the error body carries the `string_to_sign` the
service computed for debugging.

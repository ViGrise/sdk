# @vigrise/sdk

The ViGrise SDK for Node 18+ and browsers: VGR4 request signing, ViGrise
Accounts, ViGrise Storage and the Speech API. There is no root export; import a
subpath.

| Subpath | Contents |
| --- | --- |
| `@vigrise/sdk/auth` | `signRequest`, `signedFetch`, `resolveCredentials`, `assumeRole` |
| `@vigrise/sdk/auth/browser` | Web Crypto `signRequest` and `signedFetch` |
| `@vigrise/sdk/accounts` | URL builders, SSO state, tab sync (ESM) |
| `@vigrise/sdk/accounts/astro` | Astro SSO middleware and endpoints (ESM) |
| `@vigrise/sdk/accounts/react` | `AccountMenu` (ESM; `react` is an optional peer dependency) |
| `@vigrise/sdk/accounts/server` | SSO introspection, back-channel logout, OAuth2/OIDC helpers, JWKS |
| `@vigrise/sdk/speech` | `SpeechClient`, `verifyWebhook` |
| `@vigrise/sdk/storage` | `StorageClient` (Node; browsers use presigned URLs) |

All but the three browser-facing `accounts` entries ship as ESM and CommonJS.

## Speech

```ts
import { SpeechClient, verifyWebhook } from "@vigrise/sdk/speech";

const speech = new SpeechClient(); // { endpoint, credentials } optional
const { audio, contentType } = await speech.synthesize("Xin chào", "haley-f1", { format: "mp3" });
const job = await speech.createJob({ text: "A long text", voice: "haley-f1" }, { webhookUrl });
const done = await speech.waitForJob(job.id, { timeoutMs: 60_000 });
const wav = await speech.downloadGenerationAudio(done.generationId!);

// In the webhook handler, with the raw request body:
const event = verifyWebhook(secret, request.headers, rawBody);
```

`synthesizeStream` returns the audio as a `ReadableStream`. Errors extend
`SpeechError`: `SpeechServiceError` (`status`, `code`, `resourceId`, `details`),
`SpeechTransportError`, `SpeechSigningError`, `SpeechWaitTimeoutError`
(`lastJob`). Nothing is retried; pass `idempotencyKey` to repeat a call safely.
Examples: `examples/speech.mjs`, `examples/speech-tour.mjs`.

## Storage

```ts
import { StorageClient } from "@vigrise/sdk/storage";

const storage = new StorageClient();
await storage.createBucket("photos");
await storage.putObject("photos", "2026/cat.jpg", bytes, { contentType: "image/jpeg" });
await storage.uploadStream("photos", "big.bin", readable, { partSize: 8 * 1024 * 1024 });
```

Streamed uploads (`putObjectStream`, `uploadStream`) sign `UNSIGNED-PAYLOAD`.
Failures are `StorageError` with `status`, `code` and `message`.

## Accounts, server side

```ts
import { SsoIntrospector, BackchannelLogout, Jwks } from "@vigrise/sdk/accounts/server";

const introspector = new SsoIntrospector({ internalUrl, clientId, clientSecret });
const answer = await introspector.introspectActive(cookieValue); // identity, session, accounts

const logout = new BackchannelLogout({ issuer, clientId, jwks: Jwks.remote(jwksUrl) });
const { outcome, sid } = await logout.verify(logoutToken); // "fresh" or "replayed"
```

`Pkce`, `AuthorizationRequest`, `OAuth2Client` and `IdTokenVerifier` cover the
redirect flow for apps on other domains. Errors: `AccountsUnavailableError`
(retry later), `AccountsMisconfiguredError`, `InactiveSessionError`,
`InvalidTokenError`, `TokenExchangeError`, `JwksError`.

## Signing

```ts
import { signedFetch } from "@vigrise/sdk/auth";
const response = await signedFetch("https://speech.vigrise.com/v1/voices", {}, { service: "speech" });
```

Credentials come from the arguments, `VIGRISE_ACCESS_KEY_ID` and
`VIGRISE_SECRET_ACCESS_KEY` (with `VIGRISE_SESSION_TOKEN`), or the profile in
`~/.vigrise/credentials`.

## Development

```bash
npm ci && npm run typecheck && npm test && npm run build && npm run check:package
```

## License

Apache-2.0

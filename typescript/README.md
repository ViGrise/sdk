# @vigrise/sdk

The ViGrise SDK for TypeScript: VGR4 request signing, ViGrise Accounts,
ViGrise Storage and the Speech API. Requires Node 24; `/speech`, `/auth/browser`
and the browser parts of `/accounts` also run in browsers. There is no root
export; import a subpath.

```sh
npm install @vigrise/sdk
```

| Subpath | Contents | Builds |
| --- | --- | --- |
| `@vigrise/sdk/auth` | `signRequest`, `signedFetch`, `resolveCredentials`, `resolveEndpoint`, `assumeRole` | ESM, CommonJS |
| `@vigrise/sdk/auth/browser` | Web Crypto `signRequest` and `signedFetch` | ESM, CommonJS |
| `@vigrise/sdk/speech` | `SpeechClient`, `verifyWebhook` (Web Crypto, Node and browsers) | ESM, CommonJS |
| `@vigrise/sdk/storage` | `StorageClient` (Node; browsers use presigned URLs) | ESM, CommonJS |
| `@vigrise/sdk/accounts/server` | SSO introspection, back-channel logout, OAuth2/OIDC, JWKS | ESM, CommonJS |
| `@vigrise/sdk/accounts` | URL builders, SSO state, tab sync | ESM |
| `@vigrise/sdk/accounts/astro` | Astro SSO middleware and endpoints | ESM |
| `@vigrise/sdk/accounts/react` | `AccountMenu` (`react` is an optional peer dependency) | ESM |

## Guides

[Auth](../docs/auth/README.md), [Speech](../docs/speech/README.md),
[Storage](../docs/storage/README.md) and [Accounts](../docs/accounts/README.md).
Examples: `examples/speech.mjs`, `examples/speech-tour.mjs`,
`examples/browser-speech.mjs`.

## Development

```bash
npm ci && npm run typecheck && npm test && npm run build && npm run check:package
```

## License

Apache-2.0

# @vigrise/sdk

The ViGrise SDK for TypeScript: request signing, credentials and clients for
ViGrise services, each service under its own subpath. Requires Node 24; the
table notes which subpaths also run in browsers. There is no root export;
import a subpath.

```sh
npm install @vigrise/sdk
```

| Subpath                        | Contents                                                                            | Builds        |
| ------------------------------ | ----------------------------------------------------------------------------------- | ------------- |
| `@vigrise/sdk/auth`            | `signRequest`, `signedFetch`, `resolveCredentials`, `resolveEndpoint`, `assumeRole` | ESM, CommonJS |
| `@vigrise/sdk/auth/browser`    | Web Crypto `signRequest` and `signedFetch`                                          | ESM, CommonJS |
| `@vigrise/sdk/speech`          | `SpeechClient`, `verifyWebhook` (Web Crypto, Node and browsers)                     | ESM, CommonJS |
| `@vigrise/sdk/storage`         | `StorageClient` (Node; browsers use presigned URLs)                                 | ESM, CommonJS |
| `@vigrise/sdk/accounts/server` | Back-channel logout, OAuth2/OIDC, JWKS                                              | ESM, CommonJS |

## Guides

Each service has a guide with examples in [`docs/`](https://github.com/ViGrise/sdk/tree/main/docs), and runnable
examples live in [`typescript/examples/`](https://github.com/ViGrise/sdk/tree/main/typescript/examples).

## Development

```bash
npm ci && npm run build && npm run typecheck && npm test && npm run check:package
```

## License

Apache-2.0

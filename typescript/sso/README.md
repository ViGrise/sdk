# @vigrise/sso

First-party SSO for TypeScript apps on `*.vigrise.com`. This package is for
ViGrise's own apps. The app's Accounts client needs `first_party` and
`sso_introspection`, which only operators set. Apps on other domains use the
OAuth2 helpers of [`@vigrise/sdk/accounts/server`](https://github.com/ViGrise/sdk/tree/main/typescript/sdk) instead.
Requires Node 24.

```sh
npm install @vigrise/sso
```

| Subpath                | Contents                                                         | Builds        |
| ---------------------- | ---------------------------------------------------------------- | ------------- |
| `@vigrise/sso/server`  | `SsoIntrospector`, `toMenuAccount`, `SSO_HEADER`, `ssoTokenHash` | ESM, CommonJS |
| `@vigrise/sso/browser` | Account-menu URL builders, SSO state, tab sync                   | ESM           |
| `@vigrise/sso/astro`   | Astro SSO middleware and endpoints                               | ESM           |
| `@vigrise/sso/react`   | `AccountMenu` (`react` is an optional peer dependency)           | ESM           |

## Introspect the SSO cookie

`SsoIntrospector` posts the `vigrise_sso` cookie value to the internal Accounts
URL with the app's client credentials and caches answers by token hash for 30
seconds.

```ts
import { SsoIntrospector } from "@vigrise/sso/server";

const introspector = new SsoIntrospector({ internalUrl, clientId: "vigrise-drive", clientSecret });
const { identity, session, accounts } = await introspector.introspectActive(cookieValue);
```

An unknown, revoked or ended token answers `active: false` (`introspectActive`
raises `InactiveSessionError` instead). Errors split into unavailable
(`AccountsUnavailableError`: Accounts unreachable, rate limiting or failing,
retry later) and misconfigured (`AccountsMisconfiguredError`: rejected client
credentials or an unexpected answer). `invalidateSession(sid)` and
`invalidateToken(hash)` drop cached answers; the cache lives in the process.

## Browser, Astro and React

`@vigrise/sso/browser` has the account-menu URL builders, the SSO state helpers
and tab sync. `/astro` has the SSO middleware and endpoints, and `/react` has
`AccountMenu`.

## Development

```bash
npm ci && npm run build && npm run typecheck && npm test && npm run check:package
```

## License

Apache-2.0

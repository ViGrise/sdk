# @vigrise/accounts

Single sign-on, account switching and tab sync with ViGrise Accounts for Astro and React apps on `*.vigrise.com`.

- `@vigrise/accounts/astro`:
  - `createSsoMiddleware` reads the `vigrise_sso` cookie, compares its hash with the marker cookie of the app session and sets `locals.sso` to `changed` (new or switched token), `unchanged`, `signed_out` (cookie gone while the app held an SSO session) or `anonymous`.
  - `createSsoSessionEndpoint` is the `POST` endpoint the browser calls for a `changed` state. It sends the cookie value to the app backend (`POST /api/v1/auth/sso`) only in `X-ViGrise-SSO`, answers `{status: "signed_in", auth}` with `Cache-Control: no-store` and records the cookie hash in the marker cookie.
- `@vigrise/accounts`:
  - `renderSsoStateJson` and `readSsoState` carry the middleware state to the browser, and `applySsoState` acts on it (exchange, adopt, clear caches, notify other tabs).
  - `createSessionChannel` and `reloadOnSessionChange` sync tabs over a `BroadcastChannel`.
  - `switchAccountUrl`, `addAccountUrl`, `manageAccountUrl` and `signOutUrl` build the Accounts links.
- `@vigrise/accounts/react`: `AccountMenu` for the app header, with `AccountMenuList` as its presentational part. Colours follow the CSS variables `--vg-menu-bg`, `--vg-menu-fg`, `--vg-menu-border` and `--vg-menu-avatar`; labels take translations through `labels`.

## Installation

Apps depend on the package through a `file:` path and need it built once:

```sh
cd sdk/typescript/accounts && npm ci && npm run build
```

## Example

```ts
// src/middleware/auth.ts
import { createSsoMiddleware } from "@vigrise/accounts/astro"

export const handleAuth = createSsoMiddleware()
```

```ts
// src/pages/sso/session.ts
import { createSsoSessionEndpoint } from "@vigrise/accounts/astro"

const exchange = createSsoSessionEndpoint({ backendUrl: import.meta.env.API_INTERNAL_PUBLIC_URL })
export const POST = (context) => exchange(context)
```

```tsx
<AccountMenu
  accounts={accounts}
  accountsUrl="https://accounts.vigrise.com"
  returnTo={window.location.href}
  clientId="vigrise-drive"
/>
```

## License

Apache-2.0

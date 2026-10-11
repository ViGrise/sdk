# @vigrise/accounts

Single sign-on, account switching and tab sync with ViGrise Accounts for Astro and React apps on `*.vigrise.com`.

- `@vigrise/accounts/astro`: `createSsoMiddleware` reads the `vigrise_sso` cookie, compares its hash with the app session's marker cookie and sets `locals.sso`. A new or switched token is exchanged at the app backend (`POST /api/v1/auth/sso`, token in `X-ViGrise-SSO`), a missing cookie ends the app session and an unchanged hash continues it.
- `@vigrise/accounts`: `applySsoState` applies `locals.sso` in the browser, `renderSsoStateJson` and `readSsoState` carry it from the server render, `createSessionChannel` and `reloadOnSessionChange` sync tabs over a `BroadcastChannel`, and `switchAccountUrl`, `addAccountUrl`, `manageAccountUrl` and `signOutUrl` build the Accounts links.
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

export const handleAuth = createSsoMiddleware({ backendUrl: process.env.BACKEND_INTERNAL_URL! })
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

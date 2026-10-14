# @vigrise/accounts

Single sign-on, account switching and tab sync with ViGrise Accounts for Astro and React apps on `*.vigrise.com`.

- `@vigrise/accounts/astro`:
  - `createSsoMiddleware` reads the `vigrise_sso` cookie, compares its hash with the marker cookie of the app session and sets `locals.sso` to `changed` (new or switched token), `unchanged`, `signed_out` (cookie gone while the app held an SSO session) or `anonymous`.
  - `createSsoSessionEndpoint` is the `POST` endpoint the browser calls for a `changed` state. It sends the cookie value to the app backend (`POST /api/v1/auth/sso`) only in `X-ViGrise-SSO`, answers `{status: "signed_in", auth}` with `Cache-Control: no-store` and records the cookie hash in the marker cookie.
- `@vigrise/accounts`:
  - `renderSsoStateJson` and `readSsoState` carry the middleware state to the browser, and `applySsoState` acts on it (exchange, adopt, clear caches, notify other tabs).
  - `createSessionChannel` and `reloadOnSessionChange` sync tabs over a `BroadcastChannel`.
  - `switchAccountUrl`, `addAccountUrl`, `manageAccountUrl` and `signOutUrl` build the Accounts links.
- `@vigrise/accounts/react`: `AccountMenu` for the app header, with `AccountMenuList` as its presentational part. The component owns structure and behaviour (Google style layout, `role="menu"` with arrow, Home, End and Escape keys) and carries no colours or layout of its own; each app styles it through `classNames` and `data-slot` attributes. Labels take translations through `labels` (`addAnotherAccount`, `manageAccount`, `signOut`, `signInAgain`, `accounts`, `openMenu`; `addAccount` stays as an alias of `addAnotherAccount`).
- `accountMenuModel`, `displayName` and `secondaryLine` (from `@vigrise/accounts`) are the pure model behind the menu, for app tests.

### Menu layout

Current account header (avatar, primary line, secondary line, manage button), a divider, the other accounts, "Add another account", a divider and "Sign out". The current account is the one with `active: true`, or the first account when none is active.

### Display rule

The primary line is the account name when it has one, else the email. The secondary line is the email only when the primary line is the name; an expired account shows the `signInAgain` label as its secondary line. Apps give the `primary` and `secondary` slots a one-line ellipsis style.

### Slots

Every slot is an element with `data-slot="<name>"` and the class from `classNames[<name>]`:

| Slot | Element |
| --- | --- |
| `root` | wrapper of `AccountMenu` (also takes `className`) |
| `trigger` | button that opens the menu |
| `popover` | container of the opened list |
| `list` | the `ul` with `role="menu"` |
| `header` | current account block |
| `avatar` | picture or initial, in the header and in every row |
| `text` | wrapper of the primary and secondary lines |
| `primary` | name, or email without a name |
| `secondary` | email, or the sign-in-again label |
| `manage` | "Manage your ViGrise Account" link |
| `accountRow` | link that switches to another account |
| `addAccount` | "Add another account" link |
| `signOut` | sign-out link, `${accountsUrl}/signout` by default |
| `divider` | separator |

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

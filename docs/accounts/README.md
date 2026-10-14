# Accounts

ViGrise Accounts is the sign-in service. The SDK covers two jobs.

- **First-party apps on `*.vigrise.com`** read the shared `vigrise_sso` cookie
  on the server, introspect it, and issue their own sessions. Accounts calls an
  app's back-channel logout endpoint when a session ends.
- **Apps on other domains** use the OAuth2 and OpenID Connect authorization
  code flow with PKCE.

| | Rust | TypeScript | Python |
| --- | --- | --- | --- |
| Server side | `vigrise::accounts`, feature `accounts` | `@vigrise/sdk/accounts/server` | `vigrise.accounts` (extra `accounts`) |
| `axum` extractor | feature `accounts-axum` | | |
| Test signer | feature `accounts-testing` (`accounts::testing::TestSigner`) | | |
| Browser | | `@vigrise/sdk/accounts`, `/astro`, `/react` | |

## Introspect the SSO cookie

`SsoIntrospector` posts the cookie value to the internal Accounts URL with the
app's client credentials and caches answers by token hash for 30 seconds.

```rust
use vigrise::accounts::{SsoIntrospector, SsoIntrospectorConfig};

let introspector = SsoIntrospector::new(SsoIntrospectorConfig::new(
    "http://accounts-backend:8080/o/auth", "vigrise-drive", "client-secret"));
let answer = introspector.introspect("value of the vigrise_sso cookie").await?;
if answer.active {
    println!("signed in as {:?}", answer.identity.unwrap().email);
}
```

```ts
import { SsoIntrospector } from "@vigrise/sdk/accounts/server";

const introspector = new SsoIntrospector({ internalUrl, clientId: "vigrise-drive", clientSecret });
const { identity, session, accounts } = await introspector.introspectActive(cookieValue);
```

```python
from vigrise.accounts import SsoIntrospector

introspector = SsoIntrospector(internal_url, "vigrise-drive", client_secret)
answer = introspector.introspect_active(cookie_value)  # AsyncSsoIntrospector for async
```

An unknown, revoked or ended token answers `active: false`
(`introspectActive` raises `InactiveSessionError` instead). Errors split into
unavailable (Accounts unreachable, rate limiting or failing: retry later,
counted by `unavailable_count`) and misconfigured (rejected client credentials
or an unexpected answer). `invalidate_session(sid)` and `invalidate_token(hash)`
drop cached answers; the cache lives in the process.

## Back-channel logout

`BackchannelLogout` verifies the `logout_token` Accounts posts when a session
ends: signature against the JWKS, `iss`, `aud`, the logout event, the time
window and `jti` replay. A repeated `jti` of a valid token is `replayed`: answer
200 and do nothing.

```rust
let logout = BackchannelLogout::new(issuer, "vigrise-drive", Jwks::remote(jwks_url));
let outcome = logout.verify(&logout_token).await?; // Verified or Replayed
```

```ts
const logout = new BackchannelLogout({ issuer, clientId: "vigrise-drive", jwks: Jwks.remote(jwksUrl) });
const { outcome, sid } = await logout.verify(logoutToken); // "fresh" or "replayed"
```

```python
logout = BackchannelLogout(issuer, "vigrise-drive", Jwks.remote(jwks_url))
result = logout.verify(logout_token)  # result.outcome; averify() for async
```

`Jwks.remote(url)` caches the key set for ten minutes and refetches when a
token names an unknown key id (at most every 30 seconds); `Jwks.from_set(set)`
takes a fixed set.

## Authorization code flow with PKCE

```rust
use vigrise::accounts::oauth2::{AuthorizationRequest, IdTokenVerifier, OAuth2Client, Pkce, random_state};

let pkce = Pkce::generate();
let url = AuthorizationRequest { /* endpoint, client id, redirect URI, scope, state, pkce.challenge */ }.url()?;
let tokens = OAuth2Client::new(token_endpoint, client_id, secret, redirect_uri)
    .exchange_code(&code, &pkce.verifier).await?;
let claims = IdTokenVerifier::new(issuer, client_id, jwks).verify(&tokens.id_token.unwrap(), Some(&nonce)).await?;
```

```ts
import { AuthorizationRequest, IdTokenVerifier, OAuth2Client, Pkce } from "@vigrise/sdk/accounts/server";

const pkce = Pkce.generate();
const url = new AuthorizationRequest({ authorizationEndpoint, clientId, redirectUri, scope: "openid email", state, codeChallenge: pkce.challenge }).url();
const tokens = await new OAuth2Client({ tokenEndpoint, clientId, clientSecret }).exchangeCode(code, pkce.verifier, redirectUri);
const claims = await new IdTokenVerifier({ issuer, clientId, jwks }).verify(tokens.idToken!, { nonce });
```

```python
from vigrise.accounts import AuthorizationRequest, IdTokenVerifier, OAuth2Client, Pkce

pkce = Pkce.generate()
url = AuthorizationRequest(authorization_endpoint, client_id, redirect_uri, "openid email", state, pkce.challenge).url()
tokens = OAuth2Client(token_endpoint, client_id, client_secret).exchange_code(code, pkce.verifier, redirect_uri)
claims = IdTokenVerifier(issuer, client_id, jwks).verify(tokens.id_token, nonce=nonce)
```

Leave out the client secret for a public client (CLI, desktop app). ID tokens
tolerate 60 seconds of clock skew.

## Browser and Astro

`@vigrise/sdk/accounts` has the account-menu URL builders, the SSO state
helpers and tab sync. `/astro` has the SSO middleware and endpoints, and
`/react` has `AccountMenu`. These ship as ESM only.

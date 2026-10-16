# Accounts

ViGrise Accounts is the sign-in service. The SDK covers two jobs for any
application.

- **Sign in with OAuth2 and OpenID Connect**: the authorization code flow with
  PKCE, ID token verification and RP-initiated logout.
- **Back-channel logout**: Accounts calls an app's logout endpoint when a
  session ends.

| | Rust | TypeScript | Python |
| --- | --- | --- | --- |
| Server side | `vigrise::accounts`, feature `accounts` | `@vigrise/sdk/accounts/server` | `vigrise.accounts` (extra `accounts`) |
| Test signer | feature `accounts-testing` (`accounts::testing::TestSigner`) | | |

## Back-channel logout

`BackchannelLogout` verifies the `logout_token` Accounts posts when a session
ends: signature against the JWKS, `iss`, `aud`, the logout event, the time
window and `jti` replay. A repeated `jti` of a valid token is `replayed`: answer
200 and do nothing.

```rust
let logout = BackchannelLogout::new(issuer, "my-client", Jwks::remote(jwks_url));
let outcome = logout.verify(&logout_token).await?; // Verified or Replayed
```

```ts
const logout = new BackchannelLogout({ issuer, clientId: "my-client", jwks: Jwks.remote(jwksUrl) });
const { outcome, sid } = await logout.verify(logoutToken); // "fresh" or "replayed"
```

```python
logout = BackchannelLogout(issuer, "my-client", Jwks.remote(jwks_url))
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

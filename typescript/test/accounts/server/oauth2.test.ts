import { afterEach, beforeAll, describe, expect, it } from "vitest";

import {
  AuthorizationRequest,
  IdTokenVerifier,
  InvalidTokenError,
  Jwks,
  OAuth2Client,
  Pkce,
  TokenExchangeError,
  endSessionUrl,
  randomState,
} from "../../../src/accounts/server/index.js";
import { fakeServer, jsonReply, type FakeServer } from "../../helpers/fake-server.js";
import { TestSigner } from "../../helpers/signer.js";

const ISS = "https://accounts.vigrise.com";
const servers: FakeServer[] = [];
afterEach(async () => {
  while (servers.length) await servers.pop()!.close();
});

describe("PKCE and URLs", () => {
  it("derives the RFC 7636 challenge", () => {
    expect(Pkce.fromVerifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk").challenge).toBe("E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
  });

  it("generates unique, URL-safe verifiers and states", () => {
    const [a, b] = [Pkce.generate(), Pkce.generate()];
    expect(a.verifier).not.toBe(b.verifier);
    expect(a.verifier).toMatch(/^[A-Za-z0-9_-]{43}$/);
    expect(randomState()).not.toBe(randomState());
  });

  it("builds the authorization URL", () => {
    const url = new URL(
      new AuthorizationRequest({
        authorizationEndpoint: "https://accounts.vigrise.com/o/oauth2/auth",
        clientId: "vigrise-platform-console",
        redirectUri: "https://console.vigrise.com/callback",
        scope: "openid email",
        state: "st",
        codeChallenge: "ch",
        prompt: "select_account",
        maxAge: 43200,
      }).url(),
    );
    const q = url.searchParams;
    expect([q.get("response_type"), q.get("code_challenge"), q.get("code_challenge_method"), q.get("state")]).toEqual(["code", "ch", "S256", "st"]);
    expect([q.get("prompt"), q.get("max_age"), q.get("scope")]).toEqual(["select_account", "43200", "openid email"]);
    expect(q.has("login_hint")).toBe(false);
  });

  it("builds the end-session URL", () => {
    const url = new URL(endSessionUrl("https://accounts.vigrise.com/o/oauth2/sessions/logout", "app", "https://app.example/bye", { idTokenHint: "tok", state: "s" }));
    expect(Object.fromEntries(url.searchParams)).toEqual({ client_id: "app", post_logout_redirect_uri: "https://app.example/bye", id_token_hint: "tok", state: "s" });
    const bare = new URL(endSessionUrl("https://accounts.vigrise.com/logout", "app", "https://app.example/bye"));
    expect(bare.searchParams.has("id_token_hint")).toBe(false);
  });
});

describe("OAuth2Client", () => {
  it("posts the verifier and client credentials of a confidential client", async () => {
    const server = await fakeServer(() => jsonReply({ access_token: "at", id_token: "it", refresh_token: "rt", expires_in: 3600 }));
    servers.push(server);
    const client = new OAuth2Client({ tokenEndpoint: `${server.url}/token`, clientId: "app", clientSecret: "shh" });
    const tokens = await client.exchangeCode("code-1", "verifier-1", "https://app.example/cb");
    expect(tokens).toEqual({ accessToken: "at", idToken: "it", refreshToken: "rt", expiresIn: 3600 });
    const seen = server.last();
    expect([seen.method, seen.target]).toEqual(["POST", "/token"]);
    expect(seen.headers["content-type"]).toBe("application/x-www-form-urlencoded");
    expect(Object.fromEntries(new URLSearchParams(seen.text()))).toEqual({
      grant_type: "authorization_code", code: "code-1", redirect_uri: "https://app.example/cb", client_id: "app", code_verifier: "verifier-1", client_secret: "shh",
    });
  });

  it("sends no secret for a public client", async () => {
    const server = await fakeServer(() => jsonReply({ access_token: "at" }));
    servers.push(server);
    const client = new OAuth2Client({ tokenEndpoint: `${server.url}/token`, clientId: "cli" });
    expect(await client.exchangeCode("c", "v", "http://127.0.0.1/cb")).toEqual({ accessToken: "at" });
    expect(new URLSearchParams(server.last().text()).has("client_secret")).toBe(false);
  });

  it("raises TokenExchangeError with the status and body", async () => {
    const server = await fakeServer(() => jsonReply({ error: "invalid_grant" }, 400));
    servers.push(server);
    const client = new OAuth2Client({ tokenEndpoint: `${server.url}/token`, clientId: "app", clientSecret: "s" });
    const error = await client.exchangeCode("c", "v", "https://app.example/cb").catch((e) => e);
    expect(error).toBeInstanceOf(TokenExchangeError);
    expect(error.message).toContain("400");
    expect(error.message).toContain("invalid_grant");
  });

  it("raises TokenExchangeError when the endpoint is unreachable", async () => {
    const client = new OAuth2Client({ tokenEndpoint: "http://127.0.0.1:1/token", clientId: "app" });
    await expect(client.exchangeCode("c", "v", "https://app.example/cb")).rejects.toBeInstanceOf(TokenExchangeError);
  });
});

describe("IdTokenVerifier", () => {
  let signer: TestSigner;
  beforeAll(async () => {
    signer = await TestSigner.generate();
  });
  const verifier = () => new IdTokenVerifier({ issuer: ISS, clientId: "app", jwks: Jwks.fromSet(signer.jwkSet) });
  const now = () => Math.floor(Date.now() / 1000);
  const claims = (extra: Record<string, unknown> = {}) => ({ iss: ISS, aud: "app", sub: "user-1", sid: "sid-1", nonce: "n-1", iat: now(), exp: now() + 300, auth_time: now(), ...extra });

  it("verifies signature, issuer, audience and nonce", async () => {
    const result = await verifier().verify(await signer.sign(claims()), { nonce: "n-1" });
    expect(result).toMatchObject({ sub: "user-1", sid: "sid-1", nonce: "n-1" });
    await expect(verifier().verify(await signer.sign(claims()), { nonce: "other" })).rejects.toThrow(/nonce/);
    await expect(verifier().verify(await signer.sign(claims({ iss: "https://evil.example" })))).rejects.toBeInstanceOf(InvalidTokenError);
    await expect(verifier().verify(await signer.sign(claims({ aud: "someone-else" })))).rejects.toBeInstanceOf(InvalidTokenError);
    await expect(verifier().verify(await signer.sign(claims({ exp: now() - 600 })))).rejects.toBeInstanceOf(InvalidTokenError);
  });

  it("skips the nonce check when none is expected", async () => {
    expect((await verifier().verify(await signer.sign(claims()))).sub).toBe("user-1");
  });

  it("checks the audience it is given", async () => {
    const token = await signer.sign(claims({ aud: "desktop" }));
    expect((await verifier().verifyForAudience(token, "desktop")).sub).toBe("user-1");
    await expect(verifier().verify(token)).rejects.toBeInstanceOf(InvalidTokenError);
  });
});

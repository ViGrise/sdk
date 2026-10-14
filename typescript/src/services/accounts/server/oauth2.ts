import { createHash, randomBytes } from "node:crypto";

import { decodeProtectedHeader, jwtVerify } from "jose";

import { InvalidTokenError, TokenExchangeError } from "./errors.js";
import type { Jwks } from "./jwks.js";

const REQUEST_TIMEOUT_MS = 5000;
const ID_TOKEN_LEEWAY_SECONDS = 60;

const randomUrlSafe = (bytes: number) => randomBytes(bytes).toString("base64url");

/** A PKCE verifier and its S256 challenge. */
export class Pkce {
  private constructor(
    /** Secret kept by the app and sent with the code exchange. */
    readonly verifier: string,
    /** Challenge sent with the authorization request. */
    readonly challenge: string,
  ) {}

  /** Generates a 43 character verifier from 32 random bytes. */
  static generate(): Pkce {
    return Pkce.fromVerifier(randomUrlSafe(32));
  }

  /** Derives the challenge of an existing verifier. */
  static fromVerifier(verifier: string): Pkce {
    return new Pkce(verifier, createHash("sha256").update(verifier).digest("base64url"));
  }
}

/** A random `state` (or `nonce`) value. */
export function randomState(): string {
  return randomUrlSafe(32);
}

export interface AuthorizationRequestOptions {
  /** The `authorization_endpoint` of the discovery document. */
  authorizationEndpoint: string;
  clientId: string;
  /** Registered redirect URI. */
  redirectUri: string;
  /** Space separated scopes. */
  scope: string;
  /** Anti-CSRF value echoed back on the redirect. */
  state: string;
  /** S256 challenge of the PKCE verifier. */
  codeChallenge: string;
  /** `prompt` parameter such as `none` or `select_account`. */
  prompt?: string;
  /** `max_age` in seconds. */
  maxAge?: number;
  loginHint?: string;
}

/** An authorization request for the code flow with PKCE. */
export class AuthorizationRequest {
  constructor(private readonly options: AuthorizationRequestOptions) {}

  /** Builds the URL to send the browser to. */
  url(): string {
    const { options } = this;
    const url = new URL(options.authorizationEndpoint);
    url.searchParams.append("response_type", "code");
    url.searchParams.append("client_id", options.clientId);
    url.searchParams.append("redirect_uri", options.redirectUri);
    url.searchParams.append("scope", options.scope);
    url.searchParams.append("state", options.state);
    url.searchParams.append("code_challenge", options.codeChallenge);
    url.searchParams.append("code_challenge_method", "S256");
    if (options.prompt !== undefined) url.searchParams.append("prompt", options.prompt);
    if (options.maxAge !== undefined) url.searchParams.append("max_age", String(options.maxAge));
    if (options.loginHint !== undefined) url.searchParams.append("login_hint", options.loginHint);
    return url.toString();
  }
}

/** Builds the RP-initiated logout URL. */
export function endSessionUrl(
  endSessionEndpoint: string,
  clientId: string,
  postLogoutRedirectUri: string,
  options: { idTokenHint?: string; state?: string } = {},
): string {
  const url = new URL(endSessionEndpoint);
  url.searchParams.append("client_id", clientId);
  url.searchParams.append("post_logout_redirect_uri", postLogoutRedirectUri);
  if (options.idTokenHint !== undefined) url.searchParams.append("id_token_hint", options.idTokenHint);
  if (options.state !== undefined) url.searchParams.append("state", options.state);
  return url.toString();
}

/** The token endpoint answer. */
export interface TokenResponse {
  accessToken: string;
  /** Present for the `openid` scope. */
  idToken?: string;
  refreshToken?: string;
  /** Access token lifetime in seconds. */
  expiresIn?: number;
}

export interface OAuth2ClientOptions {
  tokenEndpoint: string;
  clientId: string;
  /** Omit for a public client (CLI, desktop app) that proves possession of the PKCE verifier and holds no secret. */
  clientSecret?: string;
  /** Default redirect URI of `exchangeCode`. */
  redirectUri?: string;
}

/** Exchanges authorization codes at the token endpoint. */
export class OAuth2Client {
  constructor(private readonly options: OAuth2ClientOptions) {}

  /** Exchanges `code` with the PKCE `verifier` of the authorization request. */
  async exchangeCode(
    code: string,
    verifier: string,
    redirectUri: string | undefined = this.options.redirectUri,
  ): Promise<TokenResponse> {
    if (redirectUri === undefined) throw new TokenExchangeError("authorization code exchange failed: no redirect URI");
    const form = new URLSearchParams({
      grant_type: "authorization_code",
      code,
      redirect_uri: redirectUri,
      client_id: this.options.clientId,
      code_verifier: verifier,
    });
    if (this.options.clientSecret !== undefined) form.append("client_secret", this.options.clientSecret);
    let response: Response;
    try {
      response = await fetch(this.options.tokenEndpoint, {
        method: "POST",
        headers: { accept: "application/json", "content-type": "application/x-www-form-urlencoded" },
        body: form.toString(),
        signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
      });
    } catch (error) {
      throw new TokenExchangeError(`authorization code exchange failed: ${(error as Error).message}`, { cause: error });
    }
    if (!response.ok) {
      throw new TokenExchangeError(
        `authorization code exchange failed: status ${response.status}: ${await response.text().catch(() => "")}`,
      );
    }
    let raw: any;
    try {
      raw = await response.json();
    } catch (error) {
      throw new TokenExchangeError(`authorization code exchange failed: ${(error as Error).message}`, { cause: error });
    }
    return {
      accessToken: raw.access_token,
      ...(raw.id_token != null && { idToken: raw.id_token }),
      ...(raw.refresh_token != null && { refreshToken: raw.refresh_token }),
      ...(raw.expires_in != null && { expiresIn: raw.expires_in }),
    };
  }
}

/** Claims of a verified ID token. */
export interface IdTokenClaims {
  /** Client-facing subject. */
  sub: string;
  /** Accounts session id. */
  sid?: string;
  /** When the person authenticated, in seconds since the epoch. */
  authTime?: number;
  /** Nonce echoed from the authorization request. */
  nonce?: string;
}

export interface IdTokenVerifierOptions {
  issuer: string;
  clientId: string;
  jwks: Jwks;
}

/** Verifies ID tokens issued to one client. */
export class IdTokenVerifier {
  constructor(private readonly options: IdTokenVerifierOptions) {}

  /** Verifies signature, `iss`, `aud`, expiry and, when given, the nonce. */
  async verify(idToken: string, options: { nonce?: string } = {}): Promise<IdTokenClaims> {
    const claims = await this.verifyForAudience(idToken, this.options.clientId);
    if (options.nonce !== undefined && claims.nonce !== options.nonce)
      throw new InvalidTokenError("invalid ID token: nonce mismatch");
    return claims;
  }

  /** Verifies signature, `iss`, expiry and that the token is addressed to `audience`, for backends that accept ID tokens minted for another client such as a desktop app. */
  async verifyForAudience(idToken: string, audience: string): Promise<IdTokenClaims> {
    try {
      const header = decodeProtectedHeader(idToken);
      if (header.alg !== "RS256") throw new Error("unsupported signing algorithm");
      const { payload } = await jwtVerify(idToken, (protectedHeader) => this.options.jwks.key(protectedHeader), {
        issuer: this.options.issuer,
        audience,
        algorithms: ["RS256"],
        clockTolerance: ID_TOKEN_LEEWAY_SECONDS,
        requiredClaims: ["iss", "aud", "exp", "sub"],
      });
      return {
        sub: payload.sub as string,
        ...(payload.sid !== undefined && { sid: payload.sid as string }),
        ...(payload.auth_time !== undefined && { authTime: payload.auth_time as number }),
        ...(payload.nonce !== undefined && { nonce: payload.nonce as string }),
      };
    } catch (error) {
      if ((error as Error).name === "JwksError") throw error;
      throw new InvalidTokenError(`invalid ID token: ${(error as Error).message}`, { cause: error });
    }
  }
}

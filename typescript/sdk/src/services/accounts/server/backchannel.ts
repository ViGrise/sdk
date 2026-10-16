import { decodeProtectedHeader, jwtVerify } from "jose";

import { InvalidTokenError } from "./errors.js";
import type { Jwks } from "./jwks.js";

const EVENT = "http://schemas.openid.net/event/backchannel-logout";
const LEEWAY_SECONDS = 30;
const MAX_AGE_SECONDS = 300;

export interface BackchannelLogoutOptions {
  issuer: string;
  clientId: string;
  jwks: Jwks;
}

/** The claims of a verified `logout_token`, and whether its `jti` was seen before. */
export interface LogoutResult {
  /** `fresh` for the first delivery: end the session. `replayed` for a `jti` already processed: answer 200 and do nothing. */
  outcome: "fresh" | "replayed";
  /** Client-facing subject of the person. */
  sub?: string;
  /** Accounts session id that ended. */
  sid?: string;
  /** Unique token id. */
  jti: string;
}

/** Verifies back-channel `logout_token`s sent by Accounts: signature against the JWKS, `iss`, `aud`, the logout event, the time window and `jti` replay. */
export class BackchannelLogout {
  private readonly seen = new Map<string, number>();

  constructor(private readonly options: BackchannelLogoutOptions) {}

  async verify(logoutToken: string): Promise<LogoutResult> {
    let claims: Record<string, any>;
    try {
      const header = decodeProtectedHeader(logoutToken);
      if (header.alg !== "RS256") throw new Error("unsupported signing algorithm");
      const verified = await jwtVerify(logoutToken, (protectedHeader) => this.options.jwks.key(protectedHeader), {
        issuer: this.options.issuer,
        audience: this.options.clientId,
        algorithms: ["RS256"],
        clockTolerance: LEEWAY_SECONDS,
        requiredClaims: ["iss", "aud", "exp", "iat"],
      });
      claims = verified.payload;
    } catch (error) {
      if (error instanceof InvalidTokenError) throw error;
      const cause = error as Error;
      if (cause.name === "JwksError") throw error;
      throw new InvalidTokenError(`invalid logout token: ${cause.message}`, { cause });
    }
    const invalid = (message: string) => new InvalidTokenError(`invalid logout token: ${message}`);
    if (claims.events?.[EVENT] === undefined) throw invalid("missing backchannel-logout event");
    if (claims.nonce !== undefined) throw invalid("nonce is not allowed");
    if (claims.sid === undefined && claims.sub === undefined) throw invalid("neither sid nor sub");
    if (typeof claims.jti !== "string" || claims.jti === "") throw invalid("missing jti");
    const now = Math.floor(Date.now() / 1000);
    if (claims.iat > now + LEEWAY_SECONDS || claims.iat < now - MAX_AGE_SECONDS - LEEWAY_SECONDS) {
      throw invalid("issued outside the accepted time window");
    }
    for (const [jti, exp] of this.seen) {
      if (exp < now) this.seen.delete(jti);
    }
    const replayed = this.seen.has(claims.jti);
    this.seen.set(claims.jti, claims.exp);
    return { outcome: replayed ? "replayed" : "fresh", sub: claims.sub, sid: claims.sid, jti: claims.jti };
  }
}

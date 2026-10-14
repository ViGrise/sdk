import type { SsoAccount } from "../index.js";
import { AccountsMisconfiguredError, AccountsUnavailableError, InactiveSessionError } from "./errors.js";
import { ssoTokenHash } from "./hash.js";

const MAX_CACHE_ENTRIES = 10_000;
const REQUEST_TIMEOUT_MS = 5000;

/** The person behind an SSO session. */
export interface SsoIdentity {
  /** Accounts identity id, the `sub` apps key their users by. */
  id: string;
  email?: string;
  name?: string;
  picture?: string;
  email_verified: boolean;
}

/** An Accounts session. */
export interface SsoSession {
  /** Session id, the `sso_sid` of app sessions and the `sid` of logout tokens. */
  id: string;
  /** When the person authenticated, RFC 3339. */
  authenticated_at: string;
  /** Authenticator assurance level. */
  aal: string;
  /** When the session ends, RFC 3339. */
  expires_at: string;
}

/** The answer of the introspection endpoint. */
export interface Introspection {
  /** False for an unknown, revoked or ended token. */
  active: boolean;
  identity?: SsoIdentity;
  session?: SsoSession;
  /** Every account of the browser's group, as the account menu takes them. */
  accounts: SsoAccount[];
}

export interface ActiveIntrospection extends Introspection {
  active: true;
  identity: SsoIdentity;
  session: SsoSession;
}

export interface SsoIntrospectorOptions {
  /** Base URL of the Accounts identity paths reachable from the app server, for example `http://accounts-backend:8080/o/auth`. */
  internalUrl: string;
  /** OAuth2 client id of the app, registered with `sso_introspection`. */
  clientId: string;
  clientSecret: string;
  /** How long an answer is reused; 30000 by default. */
  cacheTtlMs?: number;
}

/** Drops the keys whose value is null, so absent fields read as undefined. */
function withoutNulls<T>(value: T): T {
  if (Array.isArray(value)) return value.map(withoutNulls) as T;
  if (value && typeof value === "object") {
    return Object.fromEntries(Object.entries(value).filter(([, entry]) => entry !== null).map(([key, entry]) => [key, withoutNulls(entry)])) as T;
  }
  return value;
}

/** Introspects `vigrise_sso` tokens against Accounts and caches the answers by token hash. */
export class SsoIntrospector {
  private readonly cache = new Map<string, { at: number; answer: Introspection }>();
  private unavailable = 0;
  private readonly cacheTtlMs: number;

  constructor(private readonly options: SsoIntrospectorOptions) {
    this.cacheTtlMs = options.cacheTtlMs ?? 30_000;
  }

  /** How many introspections failed because Accounts was unreachable or answered with a server error. Export it as a metric to alert on. */
  get unavailableCount(): number {
    return this.unavailable;
  }

  /** Introspects a `vigrise_sso` value. Failures are never cached. */
  async introspect(token: string): Promise<Introspection> {
    const key = ssoTokenHash(token);
    const cached = this.cache.get(key);
    if (cached && Date.now() - cached.at < this.cacheTtlMs) return cached.answer;
    const url = `${this.options.internalUrl.replace(/\/+$/, "")}/sessions/sso/introspect`;
    const credentials = Buffer.from(`${this.options.clientId}:${this.options.clientSecret}`).toString("base64");
    let response: Response;
    try {
      response = await fetch(url, {
        method: "POST",
        headers: { authorization: `Basic ${credentials}`, "content-type": "application/json" },
        body: JSON.stringify({ token }),
        signal: AbortSignal.timeout(REQUEST_TIMEOUT_MS),
      });
    } catch (error) {
      this.unavailable += 1;
      throw new AccountsUnavailableError(`ViGrise Accounts is unavailable: ${(error as Error).message}`, undefined, { cause: error });
    }
    const status = response.status;
    if (status === 401) throw new AccountsMisconfiguredError("ViGrise Accounts rejected the client credentials", status);
    if (status === 403) throw new AccountsMisconfiguredError("the client is not allowed to introspect SSO tokens", status);
    if (status === 429) throw new AccountsUnavailableError("the SSO introspection rate limit is exhausted", status);
    if (status >= 500) {
      this.unavailable += 1;
      throw new AccountsUnavailableError(`ViGrise Accounts is unavailable: status ${status}`, status);
    }
    if (status !== 200) throw new AccountsMisconfiguredError(`unexpected answer from ViGrise Accounts: status ${status}`, status);
    let raw: any;
    try {
      raw = withoutNulls(await response.json());
    } catch (error) {
      throw new AccountsMisconfiguredError(`unexpected answer from ViGrise Accounts: ${(error as Error).message}`, status, { cause: error });
    }
    const answer: Introspection = { ...raw, active: Boolean(raw?.active), accounts: raw?.accounts ?? [] };
    this.store(key, answer);
    return answer;
  }

  /** Like `introspect`, and raises `InactiveSessionError` for a token that is not active. */
  async introspectActive(token: string): Promise<ActiveIntrospection> {
    const answer = await this.introspect(token);
    if (!answer.active || !answer.identity || !answer.session) throw new InactiveSessionError();
    return answer as ActiveIntrospection;
  }

  /** Drops every cached answer of an Accounts session. The cache lives in this process, so other instances keep answering from their own cache until the entries expire. */
  invalidateSession(sessionId: string): void {
    for (const [key, entry] of this.cache) {
      if (entry.answer.session?.id === sessionId) this.cache.delete(key);
    }
  }

  /** Drops the cached answer of one token, given its `ssoTokenHash`. */
  invalidateToken(tokenHash: string): void {
    this.cache.delete(tokenHash);
  }

  private store(key: string, answer: Introspection): void {
    if (this.cache.size >= MAX_CACHE_ENTRIES) {
      const now = Date.now();
      for (const [entryKey, entry] of this.cache) {
        if (now - entry.at >= this.cacheTtlMs) this.cache.delete(entryKey);
      }
    }
    this.cache.set(key, { at: Date.now(), answer });
  }
}

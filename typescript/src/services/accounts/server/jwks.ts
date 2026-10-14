import { createLocalJWKSet, type JSONWebKeySet, type JWSHeaderParameters, type KeyLike } from "jose";

import { JwksError } from "./errors.js";

const CACHE_TTL_MS = 10 * 60 * 1000;
const REFETCH_COOLDOWN_MS = 30 * 1000;
const FETCH_TIMEOUT_MS = 5000;

export interface RemoteJwksOptions {
  /** Shortest time between two fetches of the set; 30 seconds by default. */
  refetchCooldownMs?: number;
}

/** Signing keys of ViGrise Accounts, either fixed or fetched from a JWKS URL and cached. */
export class Jwks {
  private set?: JSONWebKeySet;
  private fetchedAt = 0;

  private constructor(
    private readonly url: string | undefined,
    private readonly cooldownMs: number,
    fixed?: JSONWebKeySet,
  ) {
    this.set = fixed;
  }

  /** Uses a fixed key set. */
  static fromSet(set: JSONWebKeySet): Jwks {
    return new Jwks(undefined, 0, set);
  }

  /** Fetches the key set from `url`, caches it for ten minutes and refetches when a token names an unknown key id. */
  static remote(url: string, options: RemoteJwksOptions = {}): Jwks {
    return new Jwks(url, options.refetchCooldownMs ?? REFETCH_COOLDOWN_MS);
  }

  /** The key a token with this header is signed with. */
  async key(header: JWSHeaderParameters): Promise<KeyLike | Uint8Array> {
    if (this.url === undefined) return this.find(this.set!, header);
    const age = Date.now() - this.fetchedAt;
    if (this.set && age < CACHE_TTL_MS) {
      try {
        return await this.find(this.set, header);
      } catch (error) {
        if (age < this.cooldownMs) throw error;
      }
    } else if (this.set && age < this.cooldownMs) {
      return this.find(this.set, header);
    }
    const fetched = await this.fetchSet(this.url);
    this.set = fetched;
    this.fetchedAt = Date.now();
    return this.find(fetched, header);
  }

  private async find(set: JSONWebKeySet, header: JWSHeaderParameters): Promise<KeyLike | Uint8Array> {
    try {
      return await createLocalJWKSet(set)(header);
    } catch (error) {
      throw new JwksError(`no signing key for kid ${JSON.stringify(header.kid)}: ${(error as Error).message}`, { cause: error });
    }
  }

  private async fetchSet(url: string): Promise<JSONWebKeySet> {
    try {
      const response = await fetch(url, { signal: AbortSignal.timeout(FETCH_TIMEOUT_MS) });
      if (!response.ok) throw new Error(`status ${response.status}`);
      const set = (await response.json()) as JSONWebKeySet;
      if (!Array.isArray(set?.keys)) throw new Error("the answer is not a JWK set");
      return set;
    } catch (error) {
      throw new JwksError((error as Error).message, { cause: error });
    }
  }
}

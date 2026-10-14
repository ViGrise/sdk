import { canonicalRequest, type HeaderList } from "./canonical-core.js";
import type { Credentials } from "./types.js";
import { ALGORITHM, formatTimestamp, scope } from "./signing-core.js";

/** Payload hash of a request whose body streams unsigned. */
export const UNSIGNED_PAYLOAD = "UNSIGNED-PAYLOAD";

export interface SignInput {
  method: string;
  url: string | URL;
  headers?: HeaderList | Record<string, string>;
  body?: string | Uint8Array;
  /** Signed in place of the body's SHA-256; `UNSIGNED_PAYLOAD` for a streamed body. */
  payloadHash?: string;
  credentials: Credentials;
  service: string;
  now?: Date;
}

export interface SignedHeaders {
  authorization: string;
  "x-vgr-date": string;
  "x-vgr-content-sha256": string;
  "x-vgr-security-token"?: string;
}

/** Everything a signature covers, computed without cryptography. */
export interface SigningPlan {
  timestamp: string;
  date: string;
  credentialScope: string;
  signedHeaders: string[];
  canonicalRequest: string;
}

export function bodyBytes(input: SignInput): Uint8Array {
  return typeof input.body === "string" ? new TextEncoder().encode(input.body) : (input.body ?? new Uint8Array());
}

/** Signs `host`, `content-type` when present, and every `x-vgr-*` header. */
export function plan(input: SignInput, payloadHash: string): SigningPlan {
  if (input.credentials.accessKeyId.startsWith("VGST") && !input.credentials.sessionToken) {
    throw new Error("Temporary credentials require a session token");
  }
  const url = new URL(input.url);
  const headers: HeaderList = Array.isArray(input.headers) ? [...input.headers] : Object.entries(input.headers ?? {});
  const timestamp = formatTimestamp(input.now ?? new Date());
  const date = timestamp.slice(0, 8);

  const all: HeaderList = headers.filter(([n]) => !["x-vgr-date", "x-vgr-content-sha256"].includes(n.toLowerCase()));
  if (!all.some(([n]) => n.toLowerCase() === "host")) {
    // URL.host already omits a default port, as HTTP clients do.
    all.push(["host", url.host]);
  }
  all.push(["x-vgr-date", timestamp], ["x-vgr-content-sha256", payloadHash]);
  if (input.credentials.sessionToken) all.push(["x-vgr-security-token", input.credentials.sessionToken]);
  const signedHeaders = [
    ...new Set(
      all.map(([n]) => n.toLowerCase()).filter((n) => n === "host" || n === "content-type" || n.startsWith("x-vgr-")),
    ),
  ].sort();
  return {
    timestamp,
    date,
    credentialScope: scope(date, input.service),
    signedHeaders,
    canonicalRequest: canonicalRequest(input.method, url.pathname, url.search.replace(/^\?/, ""), all, signedHeaders, payloadHash),
  };
}

export function signedHeaders(input: SignInput, signing: SigningPlan, payloadHash: string, signature: string): SignedHeaders {
  const result: SignedHeaders = {
    authorization: `${ALGORITHM} Credential=${input.credentials.accessKeyId}/${signing.credentialScope}, SignedHeaders=${signing.signedHeaders.join(";")}, Signature=${signature}`,
    "x-vgr-date": signing.timestamp,
    "x-vgr-content-sha256": payloadHash,
  };
  if (input.credentials.sessionToken) result["x-vgr-security-token"] = input.credentials.sessionToken;
  return result;
}

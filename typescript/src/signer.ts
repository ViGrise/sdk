import { canonicalRequest, sha256Hex, type HeaderList } from "./canonical.js";
import type { Credentials } from "./credentials.js";
import { ALGORITHM, deriveSigningKey, formatTimestamp, scope, signature, stringToSign } from "./signing.js";

export interface SignInput {
  method: string;
  url: string | URL;
  headers?: HeaderList | Record<string, string>;
  body?: string | Uint8Array;
  credentials: Credentials;
  service: string;
  now?: Date;
}

export interface SignedHeaders {
  authorization: string;
  "x-vgr-date": string;
  "x-vgr-content-sha256": string;
}

/** Signs `host`, `content-type` when present, and every `x-vgr-*` header. */
export function signRequest(input: SignInput): SignedHeaders {
  const url = new URL(input.url);
  const headers: HeaderList = Array.isArray(input.headers) ? [...input.headers] : Object.entries(input.headers ?? {});
  const body = typeof input.body === "string" ? new TextEncoder().encode(input.body) : (input.body ?? new Uint8Array());
  const timestamp = formatTimestamp(input.now ?? new Date());
  const date = timestamp.slice(0, 8);
  const payloadHash = sha256Hex(body);

  const all: HeaderList = headers.filter(([n]) => !["x-vgr-date", "x-vgr-content-sha256"].includes(n.toLowerCase()));
  if (!all.some(([n]) => n.toLowerCase() === "host")) {
    // URL.host already omits a default port, as HTTP clients do.
    all.push(["host", url.host]);
  }
  all.push(["x-vgr-date", timestamp], ["x-vgr-content-sha256", payloadHash]);
  const signed = [
    ...new Set(
      all.map(([n]) => n.toLowerCase()).filter((n) => n === "host" || n === "content-type" || n.startsWith("x-vgr-")),
    ),
  ].sort();

  const creq = canonicalRequest(input.method, url.pathname, url.search.replace(/^\?/, ""), all, signed, payloadHash);
  const credentialScope = scope(date, input.service);
  const sts = stringToSign(timestamp, credentialScope, creq);
  const sig = signature(deriveSigningKey(input.credentials.secretAccessKey, date, input.service), sts);
  return {
    authorization: `${ALGORITHM} Credential=${input.credentials.accessKeyId}/${credentialScope}, SignedHeaders=${signed.join(";")}, Signature=${sig}`,
    "x-vgr-date": timestamp,
    "x-vgr-content-sha256": payloadHash,
  };
}

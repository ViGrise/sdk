// Entry point for browsers and other runtimes without Node built-ins. Signing
// uses Web Crypto, and credentials are always passed in: a backend obtains a
// narrowed role session with `assumeRole` and hands it to the page.
import type { Credentials, CredentialsProvider } from "./types.js";
import { signRequest } from "./webcrypto.js";

export { canonicalRequest, canonicalQuery, canonicalUri } from "./canonical-core.js";
export type { SignInput, SignedHeaders } from "./request.js";
export { ALGORITHM, TERMINATOR, formatTimestamp, scope } from "./signing-core.js";
export type { Credentials, CredentialsProvider } from "./types.js";
export { signRequest } from "./webcrypto.js";

export interface BrowserSignedFetchOptions {
  /** Catalog namespace of the service, for example `tts`. */
  service: string;
  credentials: Credentials | CredentialsProvider;
}

/** `fetch` with a VGR4 signature. Bodies must be strings or bytes: the signature covers their hash. */
export async function signedFetch(input: string | URL, init: RequestInit, options: BrowserSignedFetchOptions): Promise<Response> {
  const body = init.body;
  if (body !== undefined && body !== null && typeof body !== "string" && !(body instanceof Uint8Array)) {
    throw new Error("Streaming or form request bodies cannot be signed; pass a string or Uint8Array");
  }
  const credentials = typeof options.credentials === "function" ? await options.credentials() : options.credentials;
  const headers = new Headers(init.headers);
  const signed = await signRequest({
    method: init.method ?? "GET",
    url: input,
    headers: [...headers.entries()],
    body: body ?? undefined,
    credentials,
    service: options.service,
  });
  for (const [name, value] of Object.entries(signed)) {
    headers.set(name, value);
  }
  return fetch(input, { ...init, headers });
}

import { resolveCredentials } from "./credentials.js";
import { signRequest } from "./signer.js";
import type { Credentials, CredentialsProvider } from "./types.js";

export interface SignedFetchOptions {
  /** Catalog namespace of the service, for example `tts`. */
  service: string;
  /** Fixed credentials or a provider called before each request; resolved from the environment when omitted. */
  credentials?: Credentials | CredentialsProvider;
}

/** `fetch` with a VGR4 signature. Bodies must be strings or bytes: the signature covers their hash. */
export async function signedFetch(input: string | URL, init: RequestInit = {}, options: SignedFetchOptions): Promise<Response> {
  const body = init.body;
  if (body !== undefined && body !== null && typeof body !== "string" && !(body instanceof Uint8Array)) {
    throw new Error("Streaming or form request bodies cannot be signed; pass a string or Uint8Array");
  }
  const credentials =
    typeof options.credentials === "function" ? await options.credentials() : (options.credentials ?? resolveCredentials());
  const headers = new Headers(init.headers);
  const signed = signRequest({
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

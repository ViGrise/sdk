import { resolveCredentials, type Credentials } from "./credentials.js";
import { signRequest } from "./signer.js";

export interface SignedFetchOptions {
  /** Catalog namespace of the service, for example `tts`. */
  service: string;
  credentials?: Credentials;
}

/** `fetch` with a VGR4 signature. Bodies must be strings or bytes: the signature covers their hash. */
export async function signedFetch(input: string | URL, init: RequestInit = {}, options: SignedFetchOptions): Promise<Response> {
  const body = init.body;
  if (body !== undefined && body !== null && typeof body !== "string" && !(body instanceof Uint8Array)) {
    throw new Error("Streaming or form request bodies cannot be signed; pass a string or Uint8Array");
  }
  const headers = new Headers(init.headers);
  const signed = signRequest({
    method: init.method ?? "GET",
    url: input,
    headers: [...headers.entries()],
    body: body ?? undefined,
    credentials: options.credentials ?? resolveCredentials(),
    service: options.service,
  });
  for (const [name, value] of Object.entries(signed)) {
    headers.set(name, value);
  }
  return fetch(input, { ...init, headers });
}

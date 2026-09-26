import { bodyBytes, plan, signedHeaders, type SignInput, type SignedHeaders } from "./request.js";
import { ALGORITHM, TERMINATOR } from "./signing-core.js";

const encoder = new TextEncoder();

function hex(bytes: ArrayBuffer): string {
  return [...new Uint8Array(bytes)].map((b) => b.toString(16).padStart(2, "0")).join("");
}

async function sha256Hex(data: Uint8Array<ArrayBuffer>): Promise<string> {
  return hex(await crypto.subtle.digest("SHA-256", data));
}

async function hmac(key: Uint8Array<ArrayBuffer>, message: string): Promise<Uint8Array<ArrayBuffer>> {
  const imported = await crypto.subtle.importKey("raw", key, { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  return new Uint8Array(await crypto.subtle.sign("HMAC", imported, encoder.encode(message)));
}

/** Signs with the Web Crypto API, so it runs in browsers and other non-Node runtimes. */
export async function signRequest(input: SignInput): Promise<SignedHeaders> {
  const payloadHash = await sha256Hex(new Uint8Array(bodyBytes(input)));
  const signing = plan(input, payloadHash);
  const stringToSign = `${ALGORITHM}\n${signing.timestamp}\n${signing.credentialScope}\n${await sha256Hex(encoder.encode(signing.canonicalRequest))}`;
  let key = await hmac(encoder.encode(`VGR4${input.credentials.secretAccessKey}`), signing.date);
  key = await hmac(key, input.service);
  key = await hmac(key, TERMINATOR);
  const signature = hex((await hmac(key, stringToSign)).buffer);
  return signedHeaders(input, signing, payloadHash, signature);
}

import { sha256Hex } from "./canonical.js";
import { bodyBytes, plan, signedHeaders, type SignInput, type SignedHeaders } from "./request.js";
import { deriveSigningKey, signature, stringToSign } from "./signing.js";

export { UNSIGNED_PAYLOAD, type SignInput, type SignedHeaders } from "./request.js";

/** Signs `host`, `content-type` when present, and every `x-vgr-*` header. */
export function signRequest(input: SignInput): SignedHeaders {
  const payloadHash = input.payloadHash ?? sha256Hex(bodyBytes(input));
  const signing = plan(input, payloadHash);
  const sts = stringToSign(signing.timestamp, signing.credentialScope, signing.canonicalRequest);
  const sig = signature(deriveSigningKey(input.credentials.secretAccessKey, signing.date, input.service), sts);
  return signedHeaders(input, signing, payloadHash, sig);
}

import { createHmac } from "node:crypto";

import { sha256Hex } from "./canonical.js";
import { ALGORITHM, TERMINATOR } from "./signing-core.js";

export { ALGORITHM, TERMINATOR, formatTimestamp, scope } from "./signing-core.js";

export function stringToSign(timestamp: string, credentialScope: string, canonicalRequest: string): string {
  return `${ALGORITHM}\n${timestamp}\n${credentialScope}\n${sha256Hex(canonicalRequest)}`;
}

const hmac = (key: Uint8Array | string, message: string): Buffer => createHmac("sha256", key).update(message).digest();

export function deriveSigningKey(secretAccessKey: string, date: string, service: string): Buffer {
  return hmac(hmac(hmac(`VGR4${secretAccessKey}`, date), service), TERMINATOR);
}

export function signature(signingKey: Uint8Array, stringToSignValue: string): string {
  return hmac(signingKey, stringToSignValue).toString("hex");
}

import { createHmac } from "node:crypto";

import { sha256Hex } from "./canonical.js";

export const ALGORITHM = "VGR4-HMAC-SHA256";
export const TERMINATOR = "vgr4_request";

export function scope(date: string, service: string): string {
  return `${date}/${service}/${TERMINATOR}`;
}

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

/** `YYYYMMDDTHHMMSSZ` in UTC. */
export function formatTimestamp(date: Date): string {
  return date.toISOString().replace(/[-:]/g, "").replace(/\.\d{3}Z$/, "Z");
}

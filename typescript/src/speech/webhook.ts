import { createHmac, timingSafeEqual } from "node:crypto";

import { camelize } from "./case.js";
import { SpeechError } from "./errors.js";
import type { WebhookEvent } from "./types.js";

const SECRET_PREFIX = "whsec_";
const BASE64 = /^[A-Za-z0-9+/]*={0,2}$/;

export type WebhookFailure = "headers" | "timestamp" | "secret" | "signature" | "payload";

/** A webhook was refused; `reason` says why. */
export class WebhookVerificationError extends SpeechError {
  readonly reason: WebhookFailure;

  constructor(reason: WebhookFailure, message: string) {
    super(message);
    this.reason = reason;
  }
}

export interface VerifyWebhookOptions {
  /** How far the timestamp may be from the clock; 300 by default. */
  toleranceSeconds?: number;
  /** The time to check the timestamp against; the current time by default. */
  now?: Date;
}

type HeaderSource = Headers | Record<string, string | string[] | undefined>;

function header(headers: HeaderSource, name: string): string {
  let value: string | string[] | undefined | null;
  if (headers instanceof Headers) {
    value = headers.get(name);
  } else {
    const key = Object.keys(headers).find((candidate) => candidate.toLowerCase() === name);
    value = key === undefined ? undefined : headers[key];
  }
  const text = Array.isArray(value) ? value[0] : value;
  if (!text) throw new WebhookVerificationError("headers", `the ${name} header is missing`);
  return text;
}

/** Checks the signature headers of a webhook against the raw body. */
export function verifyWebhookSignature(secret: string, headers: HeaderSource, rawBody: string | Uint8Array, options: VerifyWebhookOptions = {}): void {
  const id = header(headers, "webhook-id");
  const timestamp = header(headers, "webhook-timestamp");
  const signatures = header(headers, "webhook-signature");
  const seconds = Number(timestamp);
  if (!Number.isInteger(seconds)) throw new WebhookVerificationError("timestamp", "the webhook timestamp is not a number");
  const now = Math.floor((options.now ?? new Date()).getTime() / 1000);
  if (Math.abs(now - seconds) > (options.toleranceSeconds ?? 300)) {
    throw new WebhookVerificationError("timestamp", "the webhook timestamp is outside the tolerance");
  }
  const encodedKey = secret.startsWith(SECRET_PREFIX) ? secret.slice(SECRET_PREFIX.length) : secret;
  if (!BASE64.test(encodedKey) || encodedKey.length % 4 !== 0) throw new WebhookVerificationError("secret", "the webhook secret is not base64");
  const expected = createHmac("sha256", Buffer.from(encodedKey, "base64"))
    .update(`${id}.${timestamp}.`)
    .update(rawBody)
    .digest();
  let matched = false;
  for (const entry of signatures.split(/\s+/)) {
    if (!entry.startsWith("v1,")) continue;
    const candidate = Buffer.from(entry.slice(3), "base64");
    if (candidate.length === expected.length && timingSafeEqual(candidate, expected)) matched = true;
  }
  if (!matched) throw new WebhookVerificationError("signature", "no webhook signature matches");
}

/** Verifies a webhook and parses its event. `rawBody` is the request body as received, before any JSON parsing. */
export function verifyWebhook(secret: string, headers: HeaderSource, rawBody: string | Uint8Array, options: VerifyWebhookOptions = {}): WebhookEvent {
  verifyWebhookSignature(secret, headers, rawBody, options);
  let body: any;
  try {
    body = JSON.parse(typeof rawBody === "string" ? rawBody : new TextDecoder().decode(rawBody));
  } catch (error) {
    throw new WebhookVerificationError("payload", `the webhook body is not JSON: ${(error as Error).message}`);
  }
  return {
    id: header(headers, "webhook-id"),
    type: body.type,
    timestamp: new Date(body.timestamp),
    data: camelize(body.data),
  };
}

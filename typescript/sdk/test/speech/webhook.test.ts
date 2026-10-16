import { createHmac } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

import { verifyWebhook, verifyWebhookSignature, WebhookVerificationError } from "../../src/services/speech/index.js";

interface Vector {
  name: string;
  secret: string;
  id: string;
  timestamp: number;
  body: string;
  signature: string;
  now: number;
  tolerance_seconds: number;
  valid: boolean;
  error?: string;
}

const { cases } = JSON.parse(
  readFileSync(join(import.meta.dirname, "../../../../test-vectors/webhooks/standard-webhooks.json"), "utf8"),
) as { cases: Vector[] };

describe("webhook signatures", () => {
  it("has the shared vectors", () => expect(cases.length).toBeGreaterThanOrEqual(10));

  for (const vector of cases) {
    it(vector.name, async () => {
      const headers = {
        "webhook-id": vector.id,
        "webhook-timestamp": String(vector.timestamp),
        "webhook-signature": vector.signature,
      };
      const verify = () =>
        verifyWebhookSignature(vector.secret, headers, vector.body, {
          toleranceSeconds: vector.tolerance_seconds,
          now: new Date(vector.now * 1000),
        });
      if (vector.valid) {
        await expect(verify()).resolves.toBeUndefined();
      } else {
        const error = await verify().catch((e) => e);
        expect(error).toBeInstanceOf(WebhookVerificationError);
        expect(error.reason).toBe(vector.error);
      }
    });
  }
});

describe("verifyWebhook", () => {
  const secret = "whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw";
  const body = JSON.stringify({
    type: "job.succeeded",
    timestamp: "2026-10-01T12:00:05Z",
    data: {
      id: "job_1",
      object: "job",
      status: "succeeded",
      progress: { pieces_done: 2, pieces_total: 2, characters_done: 10, characters: 10 },
      characters_used: 10,
      format: "wav",
      webhook_url: null,
      error: null,
      generation_id: "gen_1",
      created_by: "me",
      created_at: "2026-10-01T12:00:00Z",
      finished_at: "2026-10-01T12:00:05Z",
    },
  });
  const now = new Date("2026-10-01T12:00:06Z");
  const timestamp = Math.floor(now.getTime() / 1000);
  const sign = (text: string) =>
    "v1," +
    createHmac("sha256", Buffer.from(secret.slice(6), "base64"))
      .update(`evt_1.${timestamp}.${text}`)
      .digest("base64");
  const headers = new Headers({
    "webhook-id": "evt_1",
    "webhook-timestamp": String(timestamp),
    "webhook-signature": sign(body),
  });

  it("returns the event", async () => {
    const event = await verifyWebhook(secret, headers, body, { now });
    expect(event).toMatchObject({ id: "evt_1", type: "job.succeeded" });
    expect(event.timestamp).toEqual(new Date("2026-10-01T12:00:05Z"));
    expect(event.data).toMatchObject({ id: "job_1", status: "succeeded", generationId: "gen_1" });
  });

  it("refuses a tampered body", async () => {
    await expect(verifyWebhook(secret, headers, body.replace("job_1", "job_2"), { now })).rejects.toBeInstanceOf(
      WebhookVerificationError,
    );
  });

  it("accepts raw bytes", async () => {
    expect((await verifyWebhook(secret, headers, new TextEncoder().encode(body), { now })).id).toBe("evt_1");
  });
});

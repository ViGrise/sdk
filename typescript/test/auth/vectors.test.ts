import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

import { canonicalRequest, sha256Hex } from "../../src/auth/canonical.js";
import { deriveSigningKey, scope, signature, stringToSign } from "../../src/auth/signing.js";
import { signRequest } from "../../src/auth/signer.js";

interface Vector {
  request: { method: string; path: string; query: string; headers: [string, string][]; body: string; payload_hash?: string };
  credentials: { access_key_id: string; secret_access_key: string; session_token?: string };
  service: string;
  time: string;
  expected: {
    payload_hash: string;
    signed_headers: string;
    canonical_request: string;
    string_to_sign: string;
    signature: string;
    authorization: string;
  };
}

const dir = join(import.meta.dirname, "../../../test-vectors/vgr4");
const vectors: [string, Vector][] = readdirSync(dir)
  .filter((f) => f.endsWith(".json"))
  .sort()
  .map((f) => [f, JSON.parse(readFileSync(join(dir, f), "utf8")) as Vector]);

function parseTime(t: string): Date {
  return new Date(`${t.slice(0, 4)}-${t.slice(4, 6)}-${t.slice(6, 8)}T${t.slice(9, 11)}:${t.slice(11, 13)}:${t.slice(13, 15)}Z`);
}

describe("VGR4 vectors", () => {
  it("has vectors", () => expect(vectors.length).toBeGreaterThan(5));

  for (const [name, v] of vectors) {
    it(`primitives match ${name}`, () => {
      const payloadHash = v.request.payload_hash ?? sha256Hex(new TextEncoder().encode(v.request.body));
      expect(payloadHash).toBe(v.expected.payload_hash);
      const headers: [string, string][] = [...v.request.headers, ["x-vgr-date", v.time], ["x-vgr-content-sha256", payloadHash]];
      if (v.credentials.session_token) headers.push(["x-vgr-security-token", v.credentials.session_token]);
      const signed = v.expected.signed_headers.split(";");
      const creq = canonicalRequest(v.request.method, v.request.path, v.request.query, headers, signed, payloadHash);
      expect(creq).toBe(v.expected.canonical_request);
      const sts = stringToSign(v.time, scope(v.time.slice(0, 8), v.service), creq);
      expect(sts).toBe(v.expected.string_to_sign);
      const key = deriveSigningKey(v.credentials.secret_access_key, v.time.slice(0, 8), v.service);
      expect(signature(key, sts)).toBe(v.expected.signature);
    });

    it(`signRequest matches ${name}`, () => {
      const host = v.request.headers.find(([n]) => n.toLowerCase() === "host")![1];
      const query = v.request.query ? `?${v.request.query}` : "";
      const signed = signRequest({
        method: v.request.method,
        url: `https://${host}${v.request.path}${query}`,
        headers: v.request.headers,
        body: v.request.body,
        payloadHash: v.request.payload_hash,
        credentials: {
          accessKeyId: v.credentials.access_key_id,
          secretAccessKey: v.credentials.secret_access_key,
          sessionToken: v.credentials.session_token,
        },
        service: v.service,
        now: parseTime(v.time),
      });
      expect(signed.authorization).toBe(v.expected.authorization);
    });
  }
});

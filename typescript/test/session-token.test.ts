import { describe, expect, it } from "vitest";

import { signRequest } from "../src/signer.js";

describe("session token signing", () => {
  it("signs and returns the token header", () => {
    const signed = signRequest({
      method: "GET",
      url: "https://example.test/",
      credentials: {
        accessKeyId: "VGSTABCDEFGHIJKLMNOP",
        secretAccessKey: "secret",
        sessionToken: "sealed-token",
      },
      service: "speech",
      now: new Date("2026-09-26T10:00:00Z"),
    });
    expect(signed["x-vgr-security-token"]).toBe("sealed-token");
    expect(signed.authorization).toContain("x-vgr-security-token");
  });

  it("rejects a temporary key without a token", () => {
    expect(() => signRequest({
      method: "GET",
      url: "https://example.test/",
      credentials: { accessKeyId: "VGSTABCDEFGHIJKLMNOP", secretAccessKey: "secret" },
      service: "speech",
    })).toThrow(/session token/i);
  });
});

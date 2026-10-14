import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { describe, expect, it } from "vitest";

import { signedFetch } from "../../src/auth/fetch.js";

describe("signedFetch", () => {
  it("sends the VGR4 headers and the body", async () => {
    const seen: { headers: Record<string, unknown>; body: string }[] = [];
    const server = createServer((req, res) => {
      let body = "";
      req.on("data", (c) => (body += c));
      req.on("end", () => {
        seen.push({ headers: req.headers, body });
        res.end("ok");
      });
    });
    await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
    const { port } = server.address() as AddressInfo;
    const response = await signedFetch(
      `http://127.0.0.1:${port}/v1/audio/speech`,
      { method: "POST", headers: { "content-type": "application/json" }, body: '{"input":"xin chào"}' },
      { service: "speech", credentials: { accessKeyId: "VGAKTEST", secretAccessKey: "secret" } },
    );
    server.close();
    expect(await response.text()).toBe("ok");
    expect(seen[0].body).toBe('{"input":"xin chào"}');
    expect(String(seen[0].headers.authorization)).toMatch(
      /^VGR4-HMAC-SHA256 Credential=VGAKTEST\/\d{8}\/speech\/vgr4_request, SignedHeaders=content-type;host;x-vgr-content-sha256;x-vgr-date, Signature=[0-9a-f]{64}$/,
    );
  });

  it("rejects streaming bodies", async () => {
    const stream = new ReadableStream();
    await expect(
      signedFetch("http://127.0.0.1:9/x", { method: "POST", body: stream, duplex: "half" } as RequestInit, {
        service: "speech",
        credentials: { accessKeyId: "A", secretAccessKey: "s" },
      }),
    ).rejects.toThrow(/Streaming/);
  });
});

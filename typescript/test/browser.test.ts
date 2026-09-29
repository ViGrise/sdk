import { readdirSync, readFileSync } from "node:fs";
import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

import { signRequest, signedFetch } from "../src/browser.js";

interface Vector {
  request: { method: string; path: string; query: string; headers: [string, string][]; body: string; payload_hash?: string };
  credentials: { access_key_id: string; secret_access_key: string; session_token?: string };
  service: string;
  time: string;
  expected: { authorization: string };
}

const dir = join(import.meta.dirname, "../../test-vectors/vgr4");
const vectors: [string, Vector][] = readdirSync(dir)
  .filter((f) => f.endsWith(".json"))
  .sort()
  .map((f) => [f, JSON.parse(readFileSync(join(dir, f), "utf8")) as Vector]);

function parseTime(t: string): Date {
  return new Date(`${t.slice(0, 4)}-${t.slice(4, 6)}-${t.slice(6, 8)}T${t.slice(9, 11)}:${t.slice(11, 13)}:${t.slice(13, 15)}Z`);
}

/** Every module the browser entry loads, following relative imports. */
function browserModules(entry: string): string[] {
  const seen = new Set<string>();
  const visit = (file: string) => {
    if (seen.has(file)) return;
    seen.add(file);
    const source = readFileSync(join(import.meta.dirname, "../src", file), "utf8");
    for (const [, target] of source.matchAll(/from\s+"\.\/([^"]+)\.js"/g)) visit(`${target}.ts`);
  };
  visit(entry);
  return [...seen];
}

describe("browser entry", () => {
  it("loads no Node modules", () => {
    for (const file of browserModules("browser.ts")) {
      const source = readFileSync(join(import.meta.dirname, "../src", file), "utf8");
      expect(source, file).not.toMatch(/from\s+"node:/);
    }
  });

  for (const [name, v] of vectors) {
    it(`Web Crypto signing matches ${name}`, async () => {
      const host = v.request.headers.find(([n]) => n.toLowerCase() === "host")![1];
      const query = v.request.query ? `?${v.request.query}` : "";
      const signed = await signRequest({
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

  it("signedFetch signs with credentials from a provider", async () => {
    const seen: Record<string, unknown>[] = [];
    const server = createServer((req, res) => {
      seen.push(req.headers);
      res.end("ok");
    });
    await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
    const { port } = server.address() as AddressInfo;
    let calls = 0;
    const response = await signedFetch(`http://127.0.0.1:${port}/v1/voices`, {}, {
      service: "tts",
      credentials: async () => {
        calls += 1;
        return { accessKeyId: "VGSTABCDEFGHIJKLMNOP", secretAccessKey: "session", sessionToken: "sealed" };
      },
    });
    server.close();
    expect(await response.text()).toBe("ok");
    expect(calls).toBe(1);
    expect(seen[0]["x-vgr-security-token"]).toBe("sealed");
    expect(String(seen[0].authorization)).toContain("Credential=VGSTABCDEFGHIJKLMNOP/");
  });
});

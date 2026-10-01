import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { describe, expect, it } from "vitest";

import { assumeRole } from "../src/sts.js";
import { signedFetch } from "../src/fetch.js";

async function serve(status: number, reply: unknown) {
  const seen: { headers: Record<string, unknown>; body: string }[] = [];
  const server = createServer((req, res) => {
    let body = "";
    req.on("data", (c) => (body += c));
    req.on("end", () => {
      seen.push({ headers: req.headers, body });
      res.writeHead(status, { "content-type": "application/json" });
      res.end(JSON.stringify(reply));
    });
  });
  await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
  const { port } = server.address() as AddressInfo;
  return { endpoint: `http://127.0.0.1:${port}/api/v1/sts/assume-role`, seen, close: () => server.close() };
}

describe("assumeRole", () => {
  it("signs under the sts scope and returns session credentials", async () => {
    const sts = await serve(200, {
      credentials: {
        access_key_id: "VGSTABCDEFGHIJKLMNOP",
        secret_access_key: "session-secret",
        session_token: "v1.sealed",
        expiration: "2026-09-26T10:30:00Z",
      },
      assumed_role: { role_vrn: "vrn:vigrise:iam:p:role/speech-client", session_vrn: "vrn:vigrise:sts:p:assumed-role/speech-client/web" },
    });
    const session = await assumeRole(
      {
        roleVrn: "vrn:vigrise:iam:p:role/speech-client",
        roleSessionName: "web",
        durationSeconds: 900,
        sessionPolicy: { Version: "2026-10-01", Statement: { Effect: "Allow", Action: "speech:ListVoices", Resource: "*" } },
      },
      { endpoint: sts.endpoint, credentials: { accessKeyId: "VGAKBACKEND", secretAccessKey: "secret" } },
    );
    sts.close();
    expect(session.credentials).toEqual({
      accessKeyId: "VGSTABCDEFGHIJKLMNOP",
      secretAccessKey: "session-secret",
      sessionToken: "v1.sealed",
    });
    expect(session.expiration.toISOString()).toBe("2026-09-26T10:30:00.000Z");
    expect(session.sessionVrn).toBe("vrn:vigrise:sts:p:assumed-role/speech-client/web");
    expect(String(sts.seen[0].headers.authorization)).toMatch(/Credential=VGAKBACKEND\/\d{8}\/sts\/vgr4_request/);
    expect(JSON.parse(sts.seen[0].body)).toEqual({
      role_vrn: "vrn:vigrise:iam:p:role/speech-client",
      role_session_name: "web",
      duration_seconds: 900,
      session_policy: { Version: "2026-10-01", Statement: { Effect: "Allow", Action: "speech:ListVoices", Resource: "*" } },
    });
  });

  it("throws the STS error code and message", async () => {
    const sts = await serve(403, { error: { code: "AccessDenied", message: "not trusted" } });
    await expect(
      assumeRole(
        { roleVrn: "vrn:vigrise:iam:p:role/x", roleSessionName: "web" },
        { endpoint: sts.endpoint, credentials: { accessKeyId: "VGAKBACKEND", secretAccessKey: "secret" } },
      ),
    ).rejects.toThrow(/AccessDenied: not trusted/);
    sts.close();
  });

  it("signedFetch accepts a credentials provider", async () => {
    const target = await serve(200, {});
    await signedFetch(target.endpoint, {}, {
      service: "speech",
      credentials: () => ({ accessKeyId: "VGSTABCDEFGHIJKLMNOP", secretAccessKey: "s", sessionToken: "t" }),
    });
    target.close();
    expect(target.seen[0].headers["x-vgr-security-token"]).toBe("t");
  });
});

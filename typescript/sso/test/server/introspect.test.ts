import { afterEach, describe, expect, it } from "vitest";

import {
  AccountsMisconfiguredError,
  AccountsUnavailableError,
  InactiveSessionError,
  SSO_HEADER,
  SsoIntrospector,
  ssoTokenHash,
  toMenuAccount,
} from "../../src/server/index.js";
import { fakeServer, jsonReply, type FakeServer, type FakeReply, type SeenRequest } from "../helpers/fake-server.js";

const active = {
  active: true,
  identity: {
    id: "11111111-1111-1111-1111-111111111111",
    email: "a@example.com",
    name: "Ada",
    picture: null,
    email_verified: true,
  },
  session: { id: "sid-1", authenticated_at: "2026-10-11T10:00:00Z", aal: "aal1", expires_at: "2026-10-12T10:00:00Z" },
  accounts: [
    {
      session_id: "sid-1",
      identity_id: "11111111-1111-1111-1111-111111111111",
      name: "Ada",
      email: "a@example.com",
      picture: null,
      active: true,
      expired: false,
    },
  ],
};

const servers: FakeServer[] = [];
afterEach(async () => {
  while (servers.length) await servers.pop()!.close();
});

async function start(respond: (request: SeenRequest) => FakeReply, options: { cacheTtlMs?: number } = {}) {
  const server = await fakeServer(respond);
  servers.push(server);
  const introspector = new SsoIntrospector({
    internalUrl: server.url,
    clientId: "drive",
    clientSecret: "secret",
    ...options,
  });
  return { server, introspector };
}

const good = (request: SeenRequest) => jsonReply(request.json().token === "good-token" ? active : { active: false });

describe("SsoIntrospector", () => {
  it("exports the header name and the token hash", async () => {
    expect(SSO_HEADER).toBe("x-vigrise-sso");
    expect(await ssoTokenHash("abc")).toBe("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
  });

  it("returns identity, session and accounts of an active token", async () => {
    const { server, introspector } = await start(good);
    const answer = await introspector.introspect("good-token");
    expect(answer.active).toBe(true);
    expect(answer.identity).toEqual({
      id: "11111111-1111-1111-1111-111111111111",
      email: "a@example.com",
      name: "Ada",
      emailVerified: true,
    });
    expect(answer.session).toEqual({
      id: "sid-1",
      authenticatedAt: new Date("2026-10-11T10:00:00Z"),
      aal: "aal1",
      expiresAt: new Date("2026-10-12T10:00:00Z"),
    });
    expect(answer.accounts).toEqual([
      {
        sessionId: "sid-1",
        identityId: "11111111-1111-1111-1111-111111111111",
        name: "Ada",
        email: "a@example.com",
        active: true,
        expired: false,
      },
    ]);
    expect(toMenuAccount(answer.accounts[0]!)).toEqual({
      session_id: "sid-1",
      identity_id: "11111111-1111-1111-1111-111111111111",
      name: "Ada",
      email: "a@example.com",
      picture: undefined,
      active: true,
      expired: false,
    });
    const seen = server.last();
    expect([seen.method, seen.target]).toEqual(["POST", "/sessions/sso/introspect"]);
    expect(seen.headers.authorization).toBe(`Basic ${Buffer.from("drive:secret").toString("base64")}`);
  });

  it("answers inactive for an unknown token", async () => {
    const { introspector } = await start(good);
    const answer = await introspector.introspect("other");
    expect(answer.active).toBe(false);
    expect(answer.identity).toBeUndefined();
    expect(answer.accounts).toEqual([]);
  });

  it("introspectActive refuses an inactive token", async () => {
    const { introspector } = await start(good);
    await expect(introspector.introspectActive("other")).rejects.toBeInstanceOf(InactiveSessionError);
    const answer = await introspector.introspectActive("good-token");
    expect(answer.identity.id).toBe("11111111-1111-1111-1111-111111111111");
    expect(answer.session.id).toBe("sid-1");
  });

  it("caches answers per token until invalidated", async () => {
    const { server, introspector } = await start(good);
    await introspector.introspect("good-token");
    await introspector.introspect("good-token");
    expect(server.requests).toHaveLength(1);
    introspector.invalidateSession("sid-1");
    await introspector.introspect("good-token");
    expect(server.requests).toHaveLength(2);
    introspector.invalidateToken(await ssoTokenHash("good-token"));
    await introspector.introspect("good-token");
    expect(server.requests).toHaveLength(3);
  });

  it("hands out copies, so callers cannot change what is cached", async () => {
    const { introspector } = await start(good);
    const first = await introspector.introspect("good-token");
    first.identity!.name = "Mallory";
    first.accounts.length = 0;
    const second = await introspector.introspect("good-token");
    expect(second.identity!.name).toBe("Ada");
    expect(second.accounts).toHaveLength(1);
  });

  it("expires cache entries after the TTL", async () => {
    const { server, introspector } = await start(good, { cacheTtlMs: 30 });
    await introspector.introspect("good-token");
    await new Promise((resolve) => setTimeout(resolve, 60));
    await introspector.introspect("good-token");
    expect(server.requests).toHaveLength(2);
  });

  it("maps client errors to misconfiguration and server errors to unavailability, without caching them", async () => {
    const forbidden = await start(() => jsonReply({ error: "x" }, 403));
    await expect(forbidden.introspector.introspect("good-token")).rejects.toBeInstanceOf(AccountsMisconfiguredError);
    await expect(forbidden.introspector.introspect("good-token")).rejects.toBeInstanceOf(AccountsMisconfiguredError);
    expect(forbidden.server.requests).toHaveLength(2);

    const invalid = await start(() => jsonReply({ error: "invalid_client" }, 401));
    await expect(invalid.introspector.introspect("t")).rejects.toMatchObject({ status: 401 });

    const limited = await start(() => jsonReply({ error: "x" }, 429));
    await expect(limited.introspector.introspect("t")).rejects.toBeInstanceOf(AccountsUnavailableError);

    const down = await start(() => jsonReply({ error: "x" }, 502));
    await expect(down.introspector.introspect("t")).rejects.toBeInstanceOf(AccountsUnavailableError);
    expect(down.introspector.unavailableCount).toBe(1);
  });

  it("counts unreachable Accounts as unavailable", async () => {
    const introspector = new SsoIntrospector({
      internalUrl: "http://127.0.0.1:1",
      clientId: "drive",
      clientSecret: "secret",
    });
    await expect(introspector.introspect("t")).rejects.toBeInstanceOf(AccountsUnavailableError);
    expect(introspector.unavailableCount).toBe(1);
  });

  it("treats an unreadable answer as misconfiguration", async () => {
    const { introspector } = await start(() => ({ body: "<html>" }));
    await expect(introspector.introspect("t")).rejects.toBeInstanceOf(AccountsMisconfiguredError);
  });
});

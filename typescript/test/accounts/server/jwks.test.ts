import { afterEach, describe, expect, it } from "vitest";

import { Jwks, JwksError } from "../../../src/services/accounts/server/index.js";
import { fakeServer, jsonReply, type FakeServer } from "../../helpers/fake-server.js";
import { TestSigner } from "../../helpers/signer.js";

const servers: FakeServer[] = [];
afterEach(async () => {
  while (servers.length) await servers.pop()!.close();
});

describe("Jwks", () => {
  it("resolves a key of a fixed set by kid", async () => {
    const signer = await TestSigner.generate("a");
    const jwks = Jwks.fromSet(signer.jwkSet);
    expect(await jwks.key({ alg: "RS256", kid: "a" })).toBeDefined();
    await expect(jwks.key({ alg: "RS256", kid: "other" })).rejects.toBeInstanceOf(JwksError);
  });

  it("accepts a token without kid when the set has one key", async () => {
    const signer = await TestSigner.generate("a");
    expect(await Jwks.fromSet(signer.jwkSet).key({ alg: "RS256" })).toBeDefined();
  });

  it("fetches a remote set once and reuses it", async () => {
    const signer = await TestSigner.generate("a");
    const server = await fakeServer(() => jsonReply(signer.jwkSet));
    servers.push(server);
    const jwks = Jwks.remote(`${server.url}/jwks`);
    await jwks.key({ alg: "RS256", kid: "a" });
    await jwks.key({ alg: "RS256", kid: "a" });
    expect(server.requests).toHaveLength(1);
    expect(server.last().target).toBe("/jwks");
  });

  it("refetches on an unknown kid once the cooldown has passed", async () => {
    const first = await TestSigner.generate("old");
    const second = await TestSigner.generate("new");
    let current = first;
    const server = await fakeServer(() => jsonReply(current.jwkSet));
    servers.push(server);
    const jwks = Jwks.remote(`${server.url}/jwks`, { refetchCooldownMs: 0 });
    await jwks.key({ alg: "RS256", kid: "old" });
    current = second;
    expect(await jwks.key({ alg: "RS256", kid: "new" })).toBeDefined();
    expect(server.requests).toHaveLength(2);
  });

  it("does not refetch inside the cooldown", async () => {
    const signer = await TestSigner.generate("old");
    const server = await fakeServer(() => jsonReply(signer.jwkSet));
    servers.push(server);
    const jwks = Jwks.remote(`${server.url}/jwks`);
    await jwks.key({ alg: "RS256", kid: "old" });
    await expect(jwks.key({ alg: "RS256", kid: "unknown" })).rejects.toBeInstanceOf(JwksError);
    expect(server.requests).toHaveLength(1);
  });

  it("raises JwksError when the endpoint fails", async () => {
    const server = await fakeServer(() => jsonReply({}, 500));
    servers.push(server);
    await expect(Jwks.remote(`${server.url}/jwks`).key({ alg: "RS256", kid: "a" })).rejects.toBeInstanceOf(JwksError);
  });
});

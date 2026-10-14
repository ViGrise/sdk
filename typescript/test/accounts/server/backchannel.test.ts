import { beforeAll, describe, expect, it } from "vitest";

import { BackchannelLogout, InvalidTokenError, Jwks } from "../../../src/accounts/server/index.js";
import { TestSigner } from "../../helpers/signer.js";

const ISS = "https://accounts.vigrise.com";
const EVENT = "http://schemas.openid.net/event/backchannel-logout";
const now = () => Math.floor(Date.now() / 1000);

let signer: TestSigner;
let counter = 0;
beforeAll(async () => {
  signer = await TestSigner.generate();
});

const verifier = () => new BackchannelLogout({ issuer: ISS, clientId: "vigrise-drive", jwks: Jwks.fromSet(signer.jwkSet) });

function claims(): Record<string, unknown> {
  return { iss: ISS, sub: "user-1", aud: ["vigrise-drive"], iat: now(), exp: now() + 300, jti: `jti-${counter++}`, sid: "sid-1", events: { [EVENT]: {} } };
}

async function check(mutate: (claims: Record<string, unknown>) => void = () => {}) {
  const value = claims();
  mutate(value);
  return verifier().verify(await signer.sign(value));
}

describe("BackchannelLogout", () => {
  it("accepts a valid logout token and yields sid, sub and jti", async () => {
    const result = await check();
    expect(result).toMatchObject({ outcome: "fresh", sid: "sid-1", sub: "user-1" });
    expect(result.jti).toMatch(/^jti-/);
  });

  it("rejects a wrong issuer, audience or event, a nonce, and missing subjects", async () => {
    await expect(check((c) => (c.iss = "https://evil.example"))).rejects.toBeInstanceOf(InvalidTokenError);
    await expect(check((c) => (c.aud = ["vigrise-wallet"]))).rejects.toBeInstanceOf(InvalidTokenError);
    await expect(check((c) => (c.events = {}))).rejects.toThrow(/event/);
    await expect(check((c) => (c.nonce = "n"))).rejects.toThrow(/nonce/);
    await expect(check((c) => { delete c.sid; delete c.sub; })).rejects.toThrow(/sid nor sub/);
    await expect(check((c) => delete c.jti)).rejects.toThrow(/jti/);
  });

  it("enforces the time window", async () => {
    await expect(check((c) => (c.exp = now() - 600))).rejects.toBeInstanceOf(InvalidTokenError);
    await expect(check((c) => (c.iat = now() - 1200))).rejects.toThrow(/time window/);
    await expect(check((c) => (c.iat = now() + 1200))).rejects.toBeInstanceOf(InvalidTokenError);
  });

  it("rejects tampered, foreign-key and unsigned tokens", async () => {
    const token = await signer.sign(claims());
    const [header, , signature] = token.split(".");
    const other = (await signer.sign({ iss: ISS })).split(".")[1];
    await expect(verifier().verify(`${header}.${other}.${signature}`)).rejects.toBeInstanceOf(InvalidTokenError);
    const stranger = await TestSigner.generate(signer.kid);
    await expect(verifier().verify(await stranger.sign(claims()))).rejects.toBeInstanceOf(InvalidTokenError);
    await expect(verifier().verify("not-a-jwt")).rejects.toBeInstanceOf(InvalidTokenError);
  });

  it("rejects an algorithm other than RS256", async () => {
    const unsigned = `${Buffer.from('{"alg":"none"}').toString("base64url")}.${Buffer.from(JSON.stringify(claims())).toString("base64url")}.`;
    await expect(verifier().verify(unsigned)).rejects.toBeInstanceOf(InvalidTokenError);
  });

  it("reports a repeated jti as replayed", async () => {
    const instance = verifier();
    const token = await signer.sign(claims());
    expect((await instance.verify(token)).outcome).toBe("fresh");
    const second = await instance.verify(token);
    expect(second).toMatchObject({ outcome: "replayed", sid: "sid-1" });
  });
});

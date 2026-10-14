// Loads the built package by name through both module systems, as users do.
const assert = require("node:assert");

const cjs = require("@vigrise/sdk/auth");
assert.strictEqual(typeof cjs.signRequest, "function", "require() exposes signRequest");
assert.strictEqual(typeof cjs.signedFetch, "function", "require() exposes signedFetch");

for (const [subpath, name] of [
  ["auth/browser", "signRequest"],
  ["accounts/server", "BackchannelLogout"],
  ["speech", "SpeechClient"],
  ["storage", "StorageClient"],
]) {
  assert.strictEqual(typeof require(`@vigrise/sdk/${subpath}`)[name], "function", `require() exposes ${subpath} ${name}`);
}

Promise.all([
  import("@vigrise/sdk/auth"),
  import("@vigrise/sdk/accounts"),
  import("@vigrise/sdk/accounts/astro"),
  import("@vigrise/sdk/accounts/server"),
  import("@vigrise/sdk/speech"),
  import("@vigrise/sdk/storage"),
]).then(([esm]) => {
  assert.strictEqual(typeof esm.signRequest, "function", "import() exposes signRequest");
  const input = {
    method: "GET",
    url: "https://speech.vigrise.com/v1/voices",
    credentials: { accessKeyId: "VGAK", secretAccessKey: "s" },
    service: "speech",
    now: new Date("2026-10-01T12:00:00Z"),
  };
  assert.deepStrictEqual(cjs.signRequest(input), esm.signRequest(input), "both builds sign identically");
  console.log("package ok");
});

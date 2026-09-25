// Loads the built package by name through both module systems, as users do.
const assert = require("node:assert");

const cjs = require("@vigrise/auth");
assert.strictEqual(typeof cjs.signRequest, "function", "require() exposes signRequest");
assert.strictEqual(typeof cjs.signedFetch, "function", "require() exposes signedFetch");

import("@vigrise/auth").then((esm) => {
  assert.strictEqual(typeof esm.signRequest, "function", "import() exposes signRequest");
  const input = {
    method: "GET",
    url: "https://tts.vigrise.com/v1/voices",
    credentials: { accessKeyId: "VGAK", secretAccessKey: "s" },
    service: "tts",
    now: new Date("2026-10-01T12:00:00Z"),
  };
  assert.deepStrictEqual(cjs.signRequest(input), esm.signRequest(input), "both builds sign identically");
  console.log("package ok");
});

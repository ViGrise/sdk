// Loads the built package by name through both module systems, as users do.
const assert = require("node:assert");

const cjs = require("@vigrise/sso/server");
assert.strictEqual(typeof cjs.SsoIntrospector, "function", "require() exposes SsoIntrospector");
assert.strictEqual(typeof cjs.toMenuAccount, "function", "require() exposes toMenuAccount");

Promise.all([
  import("@vigrise/sso/server"),
  import("@vigrise/sso/browser"),
  import("@vigrise/sso/astro"),
  import("@vigrise/sso/react"),
]).then(async ([server, browser]) => {
  assert.strictEqual(typeof server.SsoIntrospector, "function", "import() exposes SsoIntrospector");
  assert.strictEqual(typeof browser.signOutUrl, "function", "import() exposes signOutUrl");
  assert.strictEqual(server.SSO_HEADER, cjs.SSO_HEADER, "both builds share the header name");
  assert.strictEqual(await server.ssoTokenHash("a"), await browser.ssoTokenHash("a"), "hashes agree");
  console.log("package ok");
});

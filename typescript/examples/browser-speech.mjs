// Calls the ViGrise Speech API the way a web page does: with the Web Crypto
// signer and a role session its backend obtained through `assumeRole`.
// node examples/browser-speech.mjs <gateway url> [voice]   (run `npm run build` first)
// The session comes from VIGRISE_ACCESS_KEY_ID, VIGRISE_SECRET_ACCESS_KEY and
// VIGRISE_SESSION_TOKEN here; a page receives it from its backend instead.
import { signedFetch } from "../dist/auth/browser.js";

const [base, voice] = process.argv.slice(2);
const credentials = {
  accessKeyId: process.env.VIGRISE_ACCESS_KEY_ID,
  secretAccessKey: process.env.VIGRISE_SECRET_ACCESS_KEY,
  sessionToken: process.env.VIGRISE_SESSION_TOKEN,
};
const response = voice
  ? await signedFetch(`${base}/v1/audio/speech`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ input: "Xin chào", voice, response_format: "wav" }),
    }, { service: "speech", credentials })
  : await signedFetch(`${base}/v1/voices`, {}, { service: "speech", credentials });
const body = new Uint8Array(await response.arrayBuffer());
console.log(`${response.status} ${body.length}`);

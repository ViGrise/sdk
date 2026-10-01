// Calls the ViGrise Speech API with a VGR4 signature.
// node examples/speech.mjs <gateway url> [voice]   (run `npm run build` first)
import { signedFetch } from "../dist/index.js";

const [base, voice] = process.argv.slice(2);
const response = voice
  ? await signedFetch(`${base}/v1/audio/speech`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ input: "Xin chào", voice, response_format: "wav" }),
    }, { service: "speech" })
  : await signedFetch(`${base}/v1/voices`, {}, { service: "speech" });
const body = new Uint8Array(await response.arrayBuffer());
console.log(`${response.status} ${body.length}`);

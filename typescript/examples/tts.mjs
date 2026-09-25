// Calls the ViGrise TTS gateway with a VGR4 signature.
// node examples/tts.mjs <gateway url> [voice]   (run `npm run build` first)
import { signedFetch } from "../dist/index.js";

const [base, voice] = process.argv.slice(2);
const response = voice
  ? await signedFetch(`${base}/v1/audio/speech`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ input: "Xin chào", voice, response_format: "wav" }),
    }, { service: "tts" })
  : await signedFetch(`${base}/v1/voices`, {}, { service: "tts" });
const body = new Uint8Array(await response.arrayBuffer());
console.log(`${response.status} ${body.length}`);

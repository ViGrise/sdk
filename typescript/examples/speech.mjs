// Lists the voices, or synthesizes "Xin chào", through the Speech API.
// node examples/speech.mjs <gateway url> [voice]   (run `npm run build` first)
// Prints the HTTP status, then the number of voices or the bytes of audio.
import { SpeechClient, SpeechServiceError } from "../dist/speech/index.js";

const [endpoint, voice] = process.argv.slice(2);
const speech = new SpeechClient({ endpoint });
try {
  if (voice) {
    const spoken = await speech.synthesize("Xin chào", voice, { format: "wav" });
    console.log(`200 ${spoken.audio.length}`);
  } else {
    console.log(`200 ${(await speech.listVoices()).length}`);
  }
} catch (error) {
  if (!(error instanceof SpeechServiceError)) throw error;
  console.log(`${error.status} ${error.code}`);
}

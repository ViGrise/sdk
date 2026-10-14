// Walks through the Speech API: a synthesis, a job that reports to a webhook,
// the job's audio, and the verified webhook.
// node examples/speech-tour.mjs <gateway url> <voice>   (run `npm run build` first)
// The gateway must be able to reach 127.0.0.1, where the example listens.
import { createServer } from "node:http";

import { SpeechClient, verifyWebhook } from "../dist/services/speech/index.js";

const [endpoint, voice] = process.argv.slice(2);
const speech = new SpeechClient({ endpoint });

const spoken = await speech.synthesize("Xin chào", voice);
console.log(`synthesized ${spoken.audio.length} bytes as ${spoken.generationId}`);

const { secret } = await speech.rotateWebhookSecret();
let deliver;
const delivered = new Promise((resolve) => (deliver = resolve));
const receiver = createServer((request, response) => {
  const chunks = [];
  request.on("data", (chunk) => chunks.push(chunk));
  request.on("end", async () => {
    try {
      deliver(await verifyWebhook(secret, request.headers, Buffer.concat(chunks)));
      response.writeHead(200).end();
    } catch {
      response.writeHead(400).end();
    }
  });
});
await new Promise((resolve) => receiver.listen(0, "127.0.0.1", resolve));

try {
  const job = await speech.createJob(
    { text: "Xin chào các bạn", voice },
    { webhookUrl: `http://127.0.0.1:${receiver.address().port}/hook` },
  );
  const finished = await speech.waitForJob(job.id, { pollIntervalMs: 500, timeoutMs: 60_000 });
  const audio = await speech.downloadGenerationAudio(finished.generationId);
  console.log(`job ${finished.id} ${finished.status}: ${audio.length} bytes of audio`);
  const event = await Promise.race([
    delivered,
    new Promise((_, reject) => setTimeout(() => reject(new Error("no webhook within 60 seconds")), 60_000)),
  ]);
  console.log(`webhook ${event.type} for ${event.data.id}`);
  console.log("tour ok");
} finally {
  receiver.close();
}

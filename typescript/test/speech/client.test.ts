import { afterEach, describe, expect, it } from "vitest";

import { SpeechClient, SpeechServiceError, SpeechSigningError, SpeechTransportError, SpeechWaitTimeoutError } from "../../src/speech/index.js";
import { fakeServer, jsonReply, type FakeServer } from "../helpers/fake-server.js";

const credentials = { accessKeyId: "VGAKTEST", secretAccessKey: "secret" };

const servers: FakeServer[] = [];
afterEach(async () => {
  while (servers.length) await servers.pop()!.close();
});

async function speechClient(respond: Parameters<typeof fakeServer>[0]) {
  const server = await fakeServer(respond);
  servers.push(server);
  return { server, client: new SpeechClient({ endpoint: server.url, credentials }) };
}

const job = (status: string) => ({
  id: "job_1", object: "job", status,
  progress: { pieces_done: 1, pieces_total: 2, characters_done: 5, characters: 10 },
  characters_used: 5, format: "wav", webhook_url: null, error: null, generation_id: null,
  created_by: "me", created_at: "2026-10-01T12:00:00Z", finished_at: null,
});

const generation = {
  id: "gen_1", object: "generation", source: "sync", job_id: null, voice: "haley-f1", model: "fp16",
  format: "wav", text: "Xin", characters: 3, characters_used: 3, duration_ms: 900, status: "complete",
  audio: { available: true, bytes: 44, expires_at: "2026-10-02T12:00:00Z" },
  created_by: "me", created_at: "2026-10-01T12:00:00Z",
};

describe("SpeechClient", () => {
  it("signs every call with VGR4 for speech", async () => {
    const { server, client } = await speechClient(() => jsonReply({ object: "list", data: [] }));
    await client.listVoices();
    const headers = server.last().headers;
    expect(headers.authorization).toMatch(/^VGR4-HMAC-SHA256 Credential=VGAKTEST\/\d{8}\/speech\/vgr4_request, /);
    expect(headers["x-vgr-date"]).toBeDefined();
    expect(headers["x-vgr-content-sha256"]).toBeDefined();
  });

  it("synthesizes and reads the audio headers", async () => {
    const { server, client } = await speechClient(() => ({
      headers: { "content-type": "audio/wav", "x-vgr-generation-id": "gen_9", "x-audio-sample-rate": "24000" },
      body: "RIFFaudio",
    }));
    const speech = await client.synthesize("Xin chào", "haley-f1", {
      model: "fp16", format: "mp3", storeAudio: false, idempotencyKey: "key-1",
    });
    expect(Buffer.from(speech.audio).toString()).toBe("RIFFaudio");
    expect(speech).toMatchObject({ contentType: "audio/wav", generationId: "gen_9", sampleRate: 24000 });
    const seen = server.last();
    expect([seen.method, seen.target]).toEqual(["POST", "/v1/audio/speech"]);
    expect(seen.headers["idempotency-key"]).toBe("key-1");
    expect(seen.headers["content-type"]).toBe("application/json");
    expect(seen.json()).toEqual({ input: "Xin chào", voice: "haley-f1", model: "fp16", response_format: "mp3", store_audio: false });
  });

  it("leaves out what is not set", async () => {
    const { server, client } = await speechClient(() => ({ headers: { "content-type": "audio/wav", "x-vgr-generation-id": "g" }, body: "a" }));
    const speech = await client.synthesize("hi", "haley-f1");
    expect(speech.sampleRate).toBeUndefined();
    expect(server.last().json()).toEqual({ input: "hi", voice: "haley-f1" });
    expect(server.last().headers["idempotency-key"]).toBeUndefined();
  });

  it("streams the audio with its metadata", async () => {
    const { server, client } = await speechClient(() => ({
      headers: { "content-type": "audio/pcm", "x-vgr-generation-id": "gen_s", "x-audio-sample-rate": "16000" },
      body: ["abc", "def"],
    }));
    const stream = await client.synthesizeStream("hi", "haley-f1");
    expect(stream).toMatchObject({ contentType: "audio/pcm", generationId: "gen_s", sampleRate: 16000 });
    const chunks: Uint8Array[] = [];
    for await (const chunk of stream.audio as unknown as AsyncIterable<Uint8Array>) chunks.push(chunk);
    expect(Buffer.concat(chunks).toString()).toBe("abcdef");
    expect(server.last().json().stream).toBe(true);
  });

  it("lists, reads and deletes voices", async () => {
    const { server, client } = await speechClient((request) => {
      if (request.method === "DELETE") return { status: 204 };
      if (request.target === "/v1/voices") {
        return jsonReply({ object: "list", data: [
          { id: "haley-f1", object: "voice", type: "system" },
          { id: "voice_1", object: "voice", type: "cloned", name: "Mine", duration_ms: 8000, created_by: "me", created_at: "2026-10-01T12:00:00Z" },
        ] });
      }
      return jsonReply({ id: "voice_1", object: "voice", type: "cloned", name: "Mine", duration_ms: 8000, created_by: "me", created_at: "2026-10-01T12:00:00Z" });
    });
    const voices = await client.listVoices();
    expect(voices.map((voice) => voice.type)).toEqual(["system", "cloned"]);
    expect(voices[1]).toMatchObject({ name: "Mine", durationMs: 8000 });
    expect((await client.getVoice("voice_1")).durationMs).toBe(8000);
    await client.deleteVoice("voice_1");
    expect([server.last().method, server.last().target]).toEqual(["DELETE", "/v1/voices/voice_1"]);
  });

  it("clones a voice with a multipart body", async () => {
    const { server, client } = await speechClient(() => jsonReply({ id: "voice_2", object: "voice", type: "cloned", name: "Mine", duration_ms: 8000, created_by: "me", created_at: "2026-10-01T12:00:00Z" }, 201));
    const voice = await client.cloneVoice(
      "Mine",
      { bytes: new Uint8Array([0x52, 0x49, 0x46, 0x46, 0, 1]), contentType: "audio/wav", fileName: "me.wav" },
      { statementVersion: 1, speakerName: "An", relationship: "self", agreed: true },
      { idempotencyKey: "clone-1" },
    );
    expect(voice.id).toBe("voice_2");
    const seen = server.last();
    expect([seen.method, seen.target]).toEqual(["POST", "/v1/voices"]);
    expect(seen.headers["idempotency-key"]).toBe("clone-1");
    const boundary = /^multipart\/form-data; boundary=(.+)$/.exec(String(seen.headers["content-type"]))![1]!;
    const body = seen.body.toString("latin1");
    expect(body.startsWith(`--${boundary}\r\n`)).toBe(true);
    expect(body.endsWith(`--${boundary}--\r\n`)).toBe(true);
    expect(body).toContain('Content-Disposition: form-data; name="name"\r\n\r\nMine\r\n');
    expect(body).toContain('Content-Disposition: form-data; name="reference"; filename="me.wav"\r\nContent-Type: audio/wav\r\n\r\nRIFF\u0000\u0001\r\n');
    const consent = /name="consent"\r\n\r\n(.*?)\r\n--/s.exec(body)![1]!;
    expect(JSON.parse(consent)).toEqual({ statement_version: 1, speaker_name: "An", relationship: "self", agreed: true });
  });

  it("reads the consent statement", async () => {
    const { server, client } = await speechClient(() => jsonReply({ version: 3, statement: "I agree" }));
    expect(await client.consentStatement()).toEqual({ version: 3, statement: "I agree" });
    expect(server.last().target).toBe("/v1/consent-statement");
  });

  it("lists, reads and deletes generations", async () => {
    const { server, client } = await speechClient((request) => {
      if (request.method === "DELETE") return { status: 204 };
      if (request.target.startsWith("/v1/generations?")) return jsonReply({ object: "list", data: [generation], next_cursor: "c2" });
      return jsonReply(generation);
    });
    const page = await client.listGenerations({ cursor: "c1", limit: 5, source: "job", voice: "haley f1" });
    expect(page.nextCursor).toBe("c2");
    expect(page.data[0]).toMatchObject({ id: "gen_1", characters: 3, audio: { available: true, bytes: 44 } });
    expect(server.last().target).toBe("/v1/generations?cursor=c1&limit=5&source=job&voice=haley%20f1");
    expect((await client.getGeneration("gen_1")).id).toBe("gen_1");
    await client.deleteGeneration("gen_1");
    expect(server.last().target).toBe("/v1/generations/gen_1");
    await client.deleteGeneration("gen_1", { all: true });
    expect(server.last().target).toBe("/v1/generations/gen_1?all=true");
  });

  it("downloads generation audio through its URL without a signature", async () => {
    const { server, client } = await speechClient((request) =>
      request.target.startsWith("/v1/generations/gen_1/audio")
        ? jsonReply({ url: `http://${request.headers.host}/blob/gen_1.wav`, expires_at: "2026-10-01T13:00:00Z" })
        : { body: "RIFFaudio" },
    );
    const url = await client.generationAudioUrl("gen_1");
    expect(server.last().target).toBe("/v1/generations/gen_1/audio?redirect=false");
    expect(url.url.endsWith("/blob/gen_1.wav")).toBe(true);
    expect(url.expiresAt).toBeInstanceOf(Date);
    await client.generationAudioUrl("gen_1", { download: true });
    expect(server.last().target).toBe("/v1/generations/gen_1/audio?redirect=false&download=true");
    const audio = await client.downloadGenerationAudio("gen_1");
    expect(Buffer.from(audio).toString()).toBe("RIFFaudio");
    expect(server.last().target).toBe("/blob/gen_1.wav");
    expect(server.last().headers.authorization).toBeUndefined();
  });

  it("creates jobs from text or a script", async () => {
    const { server, client } = await speechClient(() => jsonReply(job("queued"), 202));
    const created = await client.createJob(
      { text: "Xin", voice: "haley-f1" },
      { model: "fp16", format: "opus", webhookUrl: "https://example.com/hook", storeAudio: true, idempotencyKey: "job-1" },
    );
    expect(created.status).toBe("queued");
    expect(created.createdAt).toBeInstanceOf(Date);
    const seen = server.last();
    expect([seen.method, seen.target]).toEqual(["POST", "/v1/jobs"]);
    expect(seen.headers["idempotency-key"]).toBe("job-1");
    expect(seen.json()).toEqual({ text: "Xin", voice: "haley-f1", model: "fp16", response_format: "opus", webhook_url: "https://example.com/hook", store_audio: true });
    await client.createJob({ turns: [{ voice: "a", text: "one" }, { voice: "b", text: "two" }], gapMs: 250 });
    expect(server.last().json()).toEqual({ turns: [{ voice: "a", text: "one" }, { voice: "b", text: "two" }], gap_ms: 250 });
  });

  it("reads, lists and cancels jobs", async () => {
    const { server, client } = await speechClient((request) =>
      request.target.startsWith("/v1/jobs?") ? jsonReply({ object: "list", data: [job("running")], next_cursor: null }) : jsonReply(job("cancelled")),
    );
    expect((await client.getJob("job_1")).progress).toEqual({ piecesDone: 1, piecesTotal: 2, charactersDone: 5, characters: 10 });
    const page = await client.listJobs({ status: "running", limit: 2 });
    expect(page.data[0]!.status).toBe("running");
    expect(page.nextCursor).toBeUndefined();
    expect(server.last().target).toBe("/v1/jobs?limit=2&status=running");
    expect((await client.cancelJob("job_1")).status).toBe("cancelled");
    expect([server.last().method, server.last().target]).toEqual(["POST", "/v1/jobs/job_1/cancel"]);
  });

  it("waits for a job until a final state", async () => {
    const statuses = ["queued", "running", "succeeded", "failed"];
    let polls = 0;
    const { server, client } = await speechClient(() => jsonReply(job(statuses[Math.min(polls++, 3)]!)));
    const finished = await client.waitForJob("job_1", { pollIntervalMs: 5, timeoutMs: 5000 });
    expect(finished.status).toBe("succeeded");
    expect(server.requests).toHaveLength(3);
  });

  it("times out waiting with the last job", async () => {
    const { client } = await speechClient(() => jsonReply(job("running")));
    const error = await client.waitForJob("job_1", { pollIntervalMs: 5, timeoutMs: 40 }).catch((e) => e);
    expect(error).toBeInstanceOf(SpeechWaitTimeoutError);
    expect(error.lastJob.status).toBe("running");
  });

  it("lists models and rotates the webhook secret", async () => {
    const { server, client } = await speechClient((request) =>
      request.target === "/v1/models"
        ? jsonReply({ object: "list", data: [{ id: "fp16", profile: "fast" }] })
        : jsonReply({ secret: "whsec_abc", created_at: "2026-10-01T12:00:00Z" }),
    );
    expect((await client.listModels())[0]).toMatchObject({ id: "fp16", profile: "fast" });
    const secret = await client.rotateWebhookSecret();
    expect(secret.secret).toBe("whsec_abc");
    expect([server.last().method, server.last().target]).toEqual(["POST", "/v1/webhook-secret"]);
  });

  it("raises service errors with their extra fields", async () => {
    const { client } = await speechClient(() =>
      jsonReply({ error: { code: "IdempotencyReplayed", message: "already ran", resource_id: "gen_5" } }, 409),
    );
    const error = await client.synthesize("hi", "haley-f1").catch((e) => e);
    expect(error).toBeInstanceOf(SpeechServiceError);
    expect(error).toMatchObject({ status: 409, code: "IdempotencyReplayed", resourceId: "gen_5", message: "already ran" });
  });

  it("raises transport errors when the endpoint is unreachable", async () => {
    const client = new SpeechClient({ endpoint: "http://127.0.0.1:1", credentials });
    await expect(client.listVoices()).rejects.toBeInstanceOf(SpeechTransportError);
  });
});

describe("SpeechClient credentials", () => {
  it("wraps a failing provider in a signing error without sending", async () => {
    const server = await fakeServer(() => jsonReply({}));
    servers.push(server);
    const client = new SpeechClient({ endpoint: server.url, credentials: () => Promise.reject(new Error("sts down")) });
    await expect(client.listVoices()).rejects.toBeInstanceOf(SpeechSigningError);
    expect(server.requests).toHaveLength(0);
  });

  it("invalidates the provider when the service refuses the credentials", async () => {
    const server = await fakeServer(() => jsonReply({ error: { code: "InvalidClientTokenId", message: "revoked" } }, 403));
    servers.push(server);
    let invalidated = 0;
    const provider = Object.assign(() => credentials, { invalidate: () => void (invalidated += 1) });
    const client = new SpeechClient({ endpoint: server.url, credentials: provider });
    await expect(client.listVoices()).rejects.toBeInstanceOf(SpeechServiceError);
    expect(invalidated).toBe(1);
  });

  it("reads the environment when no credentials are given", async () => {
    const server = await fakeServer(() => jsonReply({ data: [] }));
    servers.push(server);
    const previous = { ...process.env };
    process.env.VIGRISE_ACCESS_KEY_ID = "VGAKENV";
    process.env.VIGRISE_SECRET_ACCESS_KEY = "s";
    try {
      await new SpeechClient({ endpoint: server.url }).listVoices();
    } finally {
      process.env = previous;
    }
    expect(String(server.last().headers.authorization)).toContain("Credential=VGAKENV/");
  });

  it("refuses a file name or content type that would break the multipart framing", async () => {
    const server = await fakeServer(() => jsonReply({}));
    servers.push(server);
    const client = new SpeechClient({ endpoint: server.url, credentials });
    const consent = { statementVersion: 1, speakerName: "An", relationship: "self" as const, agreed: true };
    await expect(client.cloneVoice("n", { bytes: new Uint8Array(1), contentType: "audio/wav\r\nX: y", fileName: "a.wav" }, consent)).rejects.toBeInstanceOf(SpeechSigningError);
    await client.cloneVoice("n", { bytes: new Uint8Array(1), contentType: "audio/wav", fileName: 'a"\r\nb.wav' }, consent).catch(() => undefined);
    expect(server.last().text()).toContain('filename="a%22%0D%0Ab.wav"');
  });
});

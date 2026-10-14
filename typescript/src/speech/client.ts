import { resolveEndpoint } from "../auth/endpoint.js";
import { signRequest } from "../auth/webcrypto.js";
import type { Credentials, CredentialsProvider } from "../auth/types.js";
import { camelize } from "../internal/case.js";
import { errorFromResponse, SpeechSigningError, SpeechTransportError, SpeechWaitTimeoutError } from "./errors.js";
import { encodeMultipart } from "./multipart.js";
import type {
  AudioUrl, CloneVoiceOptions, Consent, ConsentStatement, Generation, Job, JobInput, JobOptions, ListGenerationsOptions,
  ListJobsOptions, Model, Page, Speech, SpeechStream, SynthesizeOptions, Voice, VoiceReference, WaitForJobOptions, WebhookSecret,
} from "./types.js";

export interface SpeechClientOptions {
  /** Base URL of the Speech API; `VIGRISE_ENDPOINT_URL_SPEECH`, `VIGRISE_ENDPOINT_URL` or `https://speech-api.vigrise.com` when omitted. */
  endpoint?: string;
  /** Fixed credentials or a provider called before each request. Omitted, Node reads VIGRISE_ACCESS_KEY_ID and VIGRISE_SECRET_ACCESS_KEY; other runtimes need them passed. */
  credentials?: Credentials | CredentialsProvider;
}

interface Call {
  method?: string;
  path: string;
  query?: Record<string, string | number | boolean | undefined>;
  json?: unknown;
  body?: Uint8Array;
  contentType?: string;
  idempotencyKey?: string;
}

function target(path: string, query: Call["query"]): string {
  const pairs = Object.entries(query ?? {})
    .filter(([, value]) => value !== undefined)
    .map(([name, value]) => `${name}=${encodeURIComponent(String(value))}`);
  return pairs.length ? `${path}?${pairs.join("&")}` : path;
}

const segment = (value: string) => encodeURIComponent(value);

/** Drops the keys without a value, so the request carries only what the caller set. */
function defined(fields: Record<string, unknown>): Record<string, unknown> {
  return Object.fromEntries(Object.entries(fields).filter(([, value]) => value !== undefined));
}

/** A client for the Speech API. Every call is VGR4-signed for the `speech` service. */
export class SpeechClient {
  private readonly endpoint: string;
  private readonly credentials?: Credentials | CredentialsProvider;

  constructor(options: SpeechClientOptions = {}) {
    this.endpoint = resolveEndpoint("speech", options.endpoint);
    this.credentials = options.credentials;
  }

  private async resolveCredentials(): Promise<Credentials> {
    const source = this.credentials;
    if (typeof source === "function") return source();
    if (source) return source;
    const env = (globalThis as { process?: { env?: Record<string, string | undefined> } }).process?.env;
    if (env?.VIGRISE_ACCESS_KEY_ID && env.VIGRISE_SECRET_ACCESS_KEY) {
      return { accessKeyId: env.VIGRISE_ACCESS_KEY_ID, secretAccessKey: env.VIGRISE_SECRET_ACCESS_KEY, sessionToken: env.VIGRISE_SESSION_TOKEN };
    }
    throw new Error("no credentials: pass `credentials`, or set VIGRISE_ACCESS_KEY_ID and VIGRISE_SECRET_ACCESS_KEY");
  }

  private async send(call: Call): Promise<Response> {
    const headers: Record<string, string> = {};
    let body: string | Uint8Array | undefined;
    if (call.json !== undefined) {
      headers["content-type"] = "application/json";
      body = JSON.stringify(call.json);
    } else if (call.body !== undefined) {
      headers["content-type"] = call.contentType ?? "application/octet-stream";
      body = call.body;
    }
    if (call.idempotencyKey !== undefined) headers["idempotency-key"] = call.idempotencyKey;
    const url = this.endpoint + target(call.path, call.query);
    try {
      const credentials = await this.resolveCredentials();
      Object.assign(headers, await signRequest({ method: call.method ?? "GET", url, headers, body, credentials, service: "speech" }));
    } catch (error) {
      throw new SpeechSigningError((error as Error).message, { cause: error });
    }
    let response: Response;
    try {
      response = await fetch(url, { method: call.method ?? "GET", headers, body: body as BodyInit | undefined });
    } catch (error) {
      throw new SpeechTransportError((error as Error).message, { cause: error });
    }
    if (!response.ok) {
      if ((response.status === 401 || response.status === 403) && typeof this.credentials === "function") this.credentials.invalidate?.();
      throw errorFromResponse(response.status, await response.text());
    }
    return response;
  }

  private async json<T = any>(call: Call): Promise<T> {
    const response = await this.send(call);
    try {
      return camelize(await response.json());
    } catch (error) {
      throw new SpeechTransportError(`the answer is not JSON: ${(error as Error).message}`, { cause: error });
    }
  }

  private static audioMetadata(response: Response) {
    const rate = response.headers.get("x-audio-sample-rate");
    return {
      contentType: response.headers.get("content-type") ?? "",
      generationId: response.headers.get("x-vgr-generation-id") ?? "",
      sampleRate: rate === null ? undefined : Number(rate),
    };
  }

  private static speechBody(text: string, voice: string, options: SynthesizeOptions, stream: boolean) {
    return defined({
      input: text,
      voice,
      model: options.model,
      response_format: options.format,
      store_audio: options.storeAudio,
      stream: stream ? true : undefined,
    });
  }

  /** Synthesizes `text` in `voice` and returns the whole audio. */
  async synthesize(text: string, voice: string, options: SynthesizeOptions = {}): Promise<Speech> {
    const response = await this.send({
      method: "POST",
      path: "/v1/audio/speech",
      json: SpeechClient.speechBody(text, voice, options, false),
      idempotencyKey: options.idempotencyKey,
    });
    return { ...SpeechClient.audioMetadata(response), audio: new Uint8Array(await response.arrayBuffer()) };
  }

  /** Synthesizes `text` in `voice` and returns the audio as it is produced. */
  async synthesizeStream(text: string, voice: string, options: SynthesizeOptions = {}): Promise<SpeechStream> {
    const response = await this.send({
      method: "POST",
      path: "/v1/audio/speech",
      json: SpeechClient.speechBody(text, voice, options, true),
      idempotencyKey: options.idempotencyKey,
    });
    return { ...SpeechClient.audioMetadata(response), audio: response.body ?? new ReadableStream({ start: (controller) => controller.close() }) };
  }

  /** The built-in voices of the project and its cloned voices. */
  async listVoices(): Promise<Voice[]> {
    return (await this.json<Page<Voice>>({ path: "/v1/voices" })).data;
  }

  getVoice(id: string): Promise<Voice> {
    return this.json({ path: `/v1/voices/${segment(id)}` });
  }

  async deleteVoice(id: string): Promise<void> {
    await this.send({ method: "DELETE", path: `/v1/voices/${segment(id)}` });
  }

  /** Clones a voice from a recording of its speaker, who has given `consent`. */
  async cloneVoice(name: string, reference: VoiceReference, consent: Consent, options: CloneVoiceOptions = {}): Promise<Voice> {
    const encoder = new TextEncoder();
    let encoded: ReturnType<typeof encodeMultipart>;
    try {
      encoded = encodeMultipart([
        { name: "name", body: encoder.encode(name) },
        { name: "reference", body: reference.bytes, fileName: reference.fileName, contentType: reference.contentType },
        {
          name: "consent",
          body: encoder.encode(JSON.stringify({
            statement_version: consent.statementVersion,
            speaker_name: consent.speakerName,
            relationship: consent.relationship,
            agreed: consent.agreed,
          })),
        },
      ]);
    } catch (error) {
      throw new SpeechSigningError((error as Error).message, { cause: error });
    }
    const { contentType, body } = encoded;
    return await this.json({ method: "POST", path: "/v1/voices", body, contentType, idempotencyKey: options.idempotencyKey });
  }

  /** The statement a speaker agrees to before a voice is cloned. */
  consentStatement(): Promise<ConsentStatement> {
    return this.json({ path: "/v1/consent-statement" });
  }

  listGenerations(options: ListGenerationsOptions = {}): Promise<Page<Generation>> {
    return this.json({
      path: "/v1/generations",
      query: { cursor: options.cursor, limit: options.limit, source: options.source, voice: options.voice },
    });
  }

  getGeneration(id: string): Promise<Generation> {
    return this.json({ path: `/v1/generations/${segment(id)}` });
  }

  /** Deletes a generation; with `all`, also the other generations of its job. */
  async deleteGeneration(id: string, options: { all?: boolean } = {}): Promise<void> {
    await this.send({ method: "DELETE", path: `/v1/generations/${segment(id)}`, query: { all: options.all ? true : undefined } });
  }

  /** A temporary URL to download the audio of a generation from; with `download`, browsers save it as a file. */
  generationAudioUrl(id: string, options: { download?: boolean } = {}): Promise<AudioUrl> {
    return this.json({
      path: `/v1/generations/${segment(id)}/audio`,
      query: { redirect: false, download: options.download ? true : undefined },
    });
  }

  /** Downloads the audio of a generation. */
  async downloadGenerationAudio(id: string): Promise<Uint8Array> {
    const location = await this.generationAudioUrl(id);
    let response: Response;
    try {
      response = await fetch(location.url);
    } catch (error) {
      throw new SpeechTransportError((error as Error).message, { cause: error });
    }
    if (!response.ok) throw errorFromResponse(response.status, await response.text());
    return new Uint8Array(await response.arrayBuffer());
  }

  /** Starts a synthesis that runs in the background. */
  createJob(input: JobInput, options: JobOptions = {}): Promise<Job> {
    const content = "turns" in input ? { turns: input.turns, gap_ms: input.gapMs } : { text: input.text, voice: input.voice };
    return this.json({
      method: "POST",
      path: "/v1/jobs",
      json: defined({
        ...content,
        model: options.model,
        response_format: options.format,
        webhook_url: options.webhookUrl,
        store_audio: options.storeAudio,
      }),
      idempotencyKey: options.idempotencyKey,
    });
  }

  getJob(id: string): Promise<Job> {
    return this.json({ path: `/v1/jobs/${segment(id)}` });
  }

  listJobs(options: ListJobsOptions = {}): Promise<Page<Job>> {
    return this.json({ path: "/v1/jobs", query: { cursor: options.cursor, limit: options.limit, status: options.status } });
  }

  /** Cancels a job that has not finished. */
  cancelJob(id: string): Promise<Job> {
    return this.json({ method: "POST", path: `/v1/jobs/${segment(id)}/cancel` });
  }

  /** Polls a job until it succeeded, failed or was cancelled. On a timeout the error carries the last job. */
  async waitForJob(id: string, options: WaitForJobOptions = {}): Promise<Job> {
    const interval = options.pollIntervalMs ?? 2000;
    const started = Date.now();
    for (;;) {
      const job = await this.getJob(id);
      if (job.status === "succeeded" || job.status === "failed" || job.status === "cancelled") return job;
      if (options.timeoutMs !== undefined && Date.now() - started + interval > options.timeoutMs) throw new SpeechWaitTimeoutError(job);
      await new Promise((resolve) => setTimeout(resolve, interval));
    }
  }

  /** The speech models of the engine. */
  async listModels(): Promise<Model[]> {
    return (await this.json<Page<Model>>({ path: "/v1/models" })).data;
  }

  /** Creates the webhook secret of the project, or replaces the current one. */
  rotateWebhookSecret(): Promise<WebhookSecret> {
    return this.json({ method: "POST", path: "/v1/webhook-secret" });
  }
}

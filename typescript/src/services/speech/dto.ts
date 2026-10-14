import type { AudioFormat, GenerationSource, GenerationStatus, JobStatus, Relationship, VoiceType } from "./enums.js";

export interface Speech {
  audio: Uint8Array;
  /** Content type of the audio, such as `audio/wav`. */
  contentType: string;
  generationId: string;
  /** Samples per second, when the service reports it. */
  sampleRate?: number;
}

export interface SpeechStream {
  /** The audio as it is produced. */
  audio: ReadableStream<Uint8Array>;
  contentType: string;
  generationId: string;
  sampleRate?: number;
}

export interface Voice {
  /** Name of a built-in voice, or id of a cloned one. */
  id: string;
  type: VoiceType;
  name?: string;
  durationMs?: number;
  createdBy?: string;
  createdAt?: Date;
  consentId?: string;
}

export interface ConsentStatement {
  /** Version to send back as `statementVersion`. */
  version: number;
  statement: string;
}

export interface Consent {
  statementVersion: number;
  speakerName: string;
  relationship: Relationship;
  agreed: boolean;
}

export interface VoiceReference {
  bytes: Uint8Array;
  /** Content type of the recording, such as `audio/wav`. */
  contentType: string;
  fileName: string;
}

export interface Generation {
  id: string;
  source: GenerationSource;
  jobId?: string;
  voice: string;
  model: string;
  format: string;
  text: string;
  characters: number;
  /** Characters metered for the generation. */
  charactersUsed: number;
  durationMs: number;
  status: GenerationStatus;
  audio: { available: boolean; bytes: number; expiresAt?: Date };
  createdBy: string;
  createdAt: Date;
}

export interface AudioUrl {
  url: string;
  expiresAt: Date;
}

export interface Turn {
  voice: string;
  text: string;
}

/** One text in one voice, or a script of turns. */
export type JobInput = { text: string; voice: string } | { turns: Turn[]; gapMs?: number };

export interface Job {
  id: string;
  status: JobStatus;
  progress: { piecesDone: number; piecesTotal: number; charactersDone: number; characters: number };
  charactersUsed: number;
  format: string;
  webhookUrl?: string;
  error?: string;
  generationId?: string;
  createdBy: string;
  createdAt: Date;
  finishedAt?: Date;
}

/** A speech model, as the engine describes it. */
export interface Model {
  id: string;
  [field: string]: unknown;
}

export interface WebhookSecret {
  /** `whsec_` followed by base64. */
  secret: string;
  createdAt: Date;
}

export interface WebhookEvent {
  /** Id of the delivery, from the `webhook-id` header. */
  id: string;
  /** `job.succeeded`, `job.failed` or `job.cancelled`. */
  type: string;
  timestamp: Date;
  /** The job as it was at the event. */
  data: Job;
}

export interface SynthesizeOptions {
  model?: string;
  format?: AudioFormat;
  storeAudio?: boolean;
  /** Makes the call safe to repeat: the service runs one request per key. */
  idempotencyKey?: string;
}

export interface CloneVoiceOptions {
  idempotencyKey?: string;
}

export interface JobOptions {
  model?: string;
  format?: AudioFormat;
  /** Where the outcome is sent; verify it with `verifyWebhook`. */
  webhookUrl?: string;
  storeAudio?: boolean;
  idempotencyKey?: string;
}

export interface ListGenerationsOptions {
  cursor?: string;
  limit?: number;
  source?: GenerationSource;
  voice?: string;
}

export interface ListJobsOptions {
  cursor?: string;
  limit?: number;
  status?: JobStatus;
}

export interface WaitForJobOptions {
  /** Pause between polls; 2000 by default. */
  pollIntervalMs?: number;
  /** Gives up after this long; waits without end when omitted. */
  timeoutMs?: number;
}

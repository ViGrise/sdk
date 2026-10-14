import { parseErrorEnvelope } from "../../common/envelope.js";
import type { Job } from "./dto.js";

/** Base of every error the Speech client raises. */
export class SpeechError extends Error {
  constructor(message: string, options?: { cause?: unknown }) {
    super(message, options);
    this.name = new.target.name;
  }
}

/** The service answered with an error envelope, such as `VoiceNotFound`. */
export class SpeechServiceError extends SpeechError {
  /** HTTP status of the answer. */
  readonly status: number;
  /** Error code of the service. */
  readonly code: string;
  /** The resource an answer refers to, such as the generation an idempotency key already created. */
  readonly resourceId?: string;
  /** The other fields of the envelope, such as `quota` on limit refusals. */
  readonly details: Record<string, unknown>;

  constructor(status: number, code: string, message: string, resourceId: string | undefined, details: Record<string, unknown>) {
    super(message);
    this.status = status;
    this.code = code;
    this.resourceId = resourceId;
    this.details = details;
  }
}

/** The request could not be sent, or the answer could not be read. */
export class SpeechTransportError extends SpeechError {}

/** The request could not be signed. */
export class SpeechSigningError extends SpeechError {}

/** A job did not finish within the time given to `waitForJob`. */
export class SpeechWaitTimeoutError extends SpeechError {
  /** The job as it was at the last poll. */
  readonly lastJob: Job;

  constructor(lastJob: Job) {
    super(`job ${lastJob.id} did not finish in time`);
    this.lastJob = lastJob;
  }
}

/** The error an error response carries: `{"error": {"code", "message", ...}}`, or the status when the body is not that envelope. */
export function errorFromResponse(status: number, body: string): SpeechServiceError {
  const envelope = parseErrorEnvelope(body);
  if (envelope) return new SpeechServiceError(status, envelope.code ?? "UnknownError", envelope.message, envelope.resourceId, envelope.details);
  return new SpeechServiceError(status, "UnknownError", body, undefined, {});
}

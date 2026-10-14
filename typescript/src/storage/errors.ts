/**
 * An error of a Storage call. `status` is the HTTP status of the service's
 * answer; it is 0 and `code` is `TransportError` or `SigningError` when the
 * request failed before an answer.
 */
export class StorageError extends Error {
  readonly status: number;
  readonly code: string;

  constructor(status: number, code: string, message: string, options?: { cause?: unknown }) {
    super(message, options);
    this.name = "StorageError";
    this.status = status;
    this.code = code;
  }
}

/** The error an error response carries: `{"error": {"code", "message"}}`, or the status when the body is not that envelope. */
export function errorFromResponse(status: number, body: string): StorageError {
  try {
    const envelope = JSON.parse(body)?.error;
    if (envelope && typeof envelope.code === "string") return new StorageError(status, envelope.code, String(envelope.message ?? ""));
  } catch {
    // Not JSON: the body becomes the message.
  }
  return new StorageError(status, "UnknownError", body);
}

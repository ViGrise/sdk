/** The fields of an error envelope: `{"error": {"code", "message", ...}}`. */
export interface ErrorEnvelope {
  code?: string;
  message: string;
  /** The resource an answer refers to. */
  resourceId?: string;
  /** The fields besides `code`, `message` and `resource_id`. */
  details: Record<string, unknown>;
}

/** The envelope a response body carries; undefined when the body is not one. */
export function parseErrorEnvelope(body: string): ErrorEnvelope | undefined {
  try {
    const envelope = JSON.parse(body)?.error;
    if (envelope && typeof envelope === "object") {
      const { code, message, resource_id: resourceId, ...details } = envelope as Record<string, unknown>;
      return {
        code: typeof code === "string" ? code : undefined,
        message: typeof message === "string" ? message : "",
        resourceId: typeof resourceId === "string" ? resourceId : undefined,
        details,
      };
    }
  } catch {
    // Not JSON.
  }
  return undefined;
}

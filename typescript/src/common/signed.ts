import { resolveEndpoint } from "./endpoint.js";
import type { Credentials, CredentialsProvider } from "../auth/types.js";
import { UNSIGNED_PAYLOAD } from "../auth/request.js";
import { signRequest } from "../auth/webcrypto.js";

/** The errors a service client raises. */
export interface Failures {
  signing(message: string, cause: unknown): Error;
  transport(message: string, cause: unknown): Error;
  response(status: number, body: string): Error;
}

export type Body = string | Uint8Array | Blob | ReadableStream<Uint8Array>;

export interface SignedCall {
  method: string;
  /** Path and query, relative to the endpoint. */
  target: string;
  headers?: Record<string, string>;
  body?: Body;
  /** Length of a streamed body; the body is signed as UNSIGNED-PAYLOAD. */
  streamLength?: number;
}

export interface SignedHttpOptions {
  service: string;
  endpoint?: string;
  /** Fixed credentials or a provider called before each request. */
  credentials?: Credentials | CredentialsProvider;
  /** Credentials when none are given. */
  defaultCredentials: () => Credentials | Promise<Credentials>;
  failures: Failures;
}

/** Sends VGR4-signed requests with Web Crypto, so it runs wherever `fetch` does. */
export class SignedHttp {
  private readonly endpoint: string;

  constructor(private readonly options: SignedHttpOptions) {
    this.endpoint = resolveEndpoint(options.service, options.endpoint);
  }

  private async resolveCredentials(): Promise<Credentials> {
    const source = this.options.credentials;
    if (typeof source === "function") return source();
    return source ?? this.options.defaultCredentials();
  }

  /** Signs and sends a call. A refused credentials provider is told to replace its credentials. */
  async send(call: SignedCall): Promise<Response> {
    const { failures } = this.options;
    const headers: Record<string, string> = { ...call.headers };
    const url = this.endpoint + call.target;
    const streamed = call.streamLength !== undefined;
    try {
      const credentials = await this.resolveCredentials();
      Object.assign(
        headers,
        await signRequest({
          method: call.method,
          url,
          headers,
          body: streamed ? undefined : (call.body as string | Uint8Array | undefined),
          payloadHash: streamed ? UNSIGNED_PAYLOAD : undefined,
          credentials,
          service: this.options.service,
        }),
      );
    } catch (error) {
      throw failures.signing((error as Error).message, error);
    }
    if (streamed) headers["content-length"] = String(call.streamLength);
    let response: Response;
    try {
      response = await fetch(url, {
        method: call.method,
        headers,
        body: call.body as BodyInit | undefined,
        ...(streamed && { duplex: "half" }),
      } as RequestInit);
    } catch (error) {
      throw failures.transport((error as Error).message, error);
    }
    if (!response.ok) {
      if ((response.status === 401 || response.status === 403) && typeof this.options.credentials === "function") {
        this.options.credentials.invalidate?.();
      }
      throw failures.response(response.status, await response.text());
    }
    return response;
  }

  /** `fetch` without a signature, for URLs that carry their own authorization. */
  async fetchUnsigned(url: string): Promise<Response> {
    try {
      return await fetch(url);
    } catch (error) {
      throw this.options.failures.transport((error as Error).message, error);
    }
  }
}

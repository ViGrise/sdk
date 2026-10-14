import { resolveCredentials } from "../auth/credentials.js";
import { signRequest, UNSIGNED_PAYLOAD } from "../auth/signer.js";
import type { Credentials, CredentialsProvider } from "../auth/types.js";
import { errorFromResponse, StorageError } from "./errors.js";
import type {
  Bucket, CompletedPart, CopyOptions, LifecycleRule, ListOptions, ListPage, ListUploadsOptions, ObjectHead, PresignRequest, Presigned,
  PutOptions, PutResult, ReadOptions, StorageObject, StorageObjectStream, UploadedPart, UploadStreamOptions, UploadsPage,
} from "./types.js";

/** Endpoint of the hosted Storage API. */
export const DEFAULT_ENDPOINT = "https://storage.vigrise.com";

/** Smallest part of a multipart upload, and the default part size of `uploadStream`. */
export const DEFAULT_PART_SIZE = 5 * 1024 * 1024;

export interface StorageClientOptions {
  /** Base URL of the Storage API; `https://storage.vigrise.com` by default. */
  endpoint?: string;
  /** Fixed credentials or a provider called before each request; resolved from the environment when omitted. */
  credentials?: Credentials | CredentialsProvider;
}

type Body = string | Uint8Array | Blob | ReadableStream<Uint8Array>;

interface Call {
  method: string;
  path: string;
  query?: Record<string, string | number | undefined>;
  headers?: Record<string, string>;
  json?: unknown;
  body?: Body;
  /** Length of a streamed body; the body is signed as UNSIGNED-PAYLOAD. */
  streamLength?: number;
}

const segment = (value: string) => encodeURIComponent(value);
const keyPath = (key: string) => key.split("/").map(segment).join("/");
const compact = <T extends object>(fields: T) => Object.fromEntries(Object.entries(fields).filter(([, value]) => value !== undefined));

function withQuery(path: string, query: Call["query"]): string {
  const pairs = Object.entries(query ?? {})
    .filter(([, value]) => value !== undefined)
    .map(([name, value]) => `${name}=${encodeURIComponent(String(value))}`);
  return pairs.length ? `${path}?${pairs.join("&")}` : path;
}

function objectHeaders(options: PutOptions): Record<string, string> {
  const headers: Record<string, string> = {};
  if (options.contentType !== undefined) headers["content-type"] = options.contentType;
  for (const [name, value] of Object.entries(options.metadata ?? {})) headers[`x-vgr-meta-${name}`] = value;
  const tags = Object.entries(options.tags ?? {}).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
  if (tags.length) headers["x-vgr-tagging"] = tags.map(([name, value]) => `${segment(name)}=${segment(value)}`).join("&");
  return headers;
}

function readHead(response: Response): ObjectHead {
  const header = (name: string) => response.headers.get(name) ?? undefined;
  const range = header("content-range");
  const length = header("content-length");
  const total = range?.split("/").pop();
  const size = total !== undefined && total !== "*" ? Number(total) : length !== undefined ? Number(length) : undefined;
  const metadata: Record<string, string> = {};
  response.headers.forEach((value, name) => {
    if (name.startsWith("x-vgr-meta-")) metadata[name.slice("x-vgr-meta-".length)] = value;
  });
  return {
    contentType: header("content-type"),
    etag: header("etag")?.replace(/^"|"$/g, ""),
    size,
    metadata,
    checksumSha256: header("x-vgr-checksum-sha256"),
  };
}

function concat(chunks: Uint8Array[], length: number): Uint8Array {
  if (chunks.length === 1) return chunks[0]!;
  const joined = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    joined.set(chunk, offset);
    offset += chunk.length;
  }
  return joined;
}

const bucketOf = (raw: any): Bucket => ({ name: raw.name, projectId: raw.project_id, createdAt: raw.created_at });
const partOf = (raw: any): CompletedPart => ({ number: raw.number, etag: raw.etag });

function ruleOf(raw: any): LifecycleRule {
  return {
    id: raw.id,
    prefix: raw.prefix ?? "",
    tags: raw.tags ?? {},
    ...(raw.expire_after_days !== undefined && { expireAfterDays: raw.expire_after_days }),
    ...(raw.abort_incomplete_uploads_after_days !== undefined && { abortIncompleteUploadsAfterDays: raw.abort_incomplete_uploads_after_days }),
    enabled: raw.enabled ?? true,
  };
}

/** A client for ViGrise Storage in Node. Every call is VGR4-signed for the `storage` service; browsers use presigned URLs. */
export class StorageClient {
  private readonly endpoint: string;
  private readonly credentials?: Credentials | CredentialsProvider;

  constructor(options: StorageClientOptions = {}) {
    this.endpoint = (options.endpoint ?? DEFAULT_ENDPOINT).replace(/\/+$/, "");
    this.credentials = options.credentials;
  }

  private async send(call: Call): Promise<Response> {
    const headers: Record<string, string> = { ...call.headers };
    let body: Body | undefined = call.body;
    if (call.json !== undefined) {
      headers["content-type"] = "application/json";
      body = JSON.stringify(call.json);
    }
    const url = this.endpoint + withQuery(call.path, call.query);
    const streamed = call.streamLength !== undefined;
    const signable = streamed ? undefined : (body as string | Uint8Array | undefined);
    let signed;
    try {
      const credentials = typeof this.credentials === "function" ? await this.credentials() : resolveCredentials({ explicit: this.credentials });
      signed = signRequest({
        method: call.method,
        url,
        headers,
        body: signable,
        payloadHash: streamed ? UNSIGNED_PAYLOAD : undefined,
        credentials,
        service: "storage",
      });
    } catch (error) {
      throw new StorageError(0, "SigningError", (error as Error).message, { cause: error });
    }
    Object.assign(headers, signed);
    if (streamed) headers["content-length"] = String(call.streamLength);
    let response: Response;
    try {
      response = await fetch(url, { method: call.method, headers, body: body as BodyInit | undefined, ...(streamed && { duplex: "half" }) } as RequestInit);
    } catch (error) {
      throw new StorageError(0, "TransportError", (error as Error).message, { cause: error });
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
      return (await response.json()) as T;
    } catch (error) {
      throw new StorageError(0, "TransportError", `the answer is not JSON: ${(error as Error).message}`, { cause: error });
    }
  }

  private async empty(call: Call): Promise<void> {
    const response = await this.send(call);
    await response.arrayBuffer();
  }

  /** Creates a bucket in the caller's project. */
  async createBucket(name: string): Promise<Bucket> {
    return bucketOf(await this.json({ method: "POST", path: "/v1/buckets", json: { name } }));
  }

  async listBuckets(): Promise<Bucket[]> {
    return (await this.json({ method: "GET", path: "/v1/buckets" })).buckets.map(bucketOf);
  }

  async getBucket(name: string): Promise<Bucket> {
    return bucketOf(await this.json({ method: "GET", path: `/v1/buckets/${segment(name)}` }));
  }

  async deleteBucket(name: string): Promise<void> {
    await this.empty({ method: "DELETE", path: `/v1/buckets/${segment(name)}` });
  }

  private objectPath(bucket: string, key: string) {
    return `/v1/buckets/${segment(bucket)}/objects/${keyPath(key)}`;
  }

  /** Uploads `body` in one request, signed with its hash. */
  async putObject(bucket: string, key: string, body: Uint8Array | string, options: PutOptions = {}): Promise<PutResult> {
    return this.json({ method: "PUT", path: this.objectPath(bucket, key), headers: objectHeaders(options), body });
  }

  /** Uploads `length` bytes from `body` in one streamed request, signed as UNSIGNED-PAYLOAD. */
  async putObjectStream(bucket: string, key: string, body: ReadableStream<Uint8Array> | Blob, length: number, options: PutOptions = {}): Promise<PutResult> {
    return this.json({ method: "PUT", path: this.objectPath(bucket, key), headers: objectHeaders(options), body, streamLength: length });
  }

  private read(method: string, bucket: string, key: string, options: ReadOptions): Promise<Response> {
    const headers = options.range ? { range: `bytes=${options.range[0]}-${options.range[1]}` } : undefined;
    return this.send({ method, path: this.objectPath(bucket, key), headers });
  }

  /** Downloads an object, or the inclusive byte range of it. */
  async getObject(bucket: string, key: string, options: ReadOptions = {}): Promise<StorageObject> {
    const response = await this.read("GET", bucket, key, options);
    return { ...readHead(response), body: new Uint8Array(await response.arrayBuffer()) };
  }

  /** Downloads an object, or the inclusive byte range of it, as a stream. */
  async getObjectStream(bucket: string, key: string, options: ReadOptions = {}): Promise<StorageObjectStream> {
    const response = await this.read("GET", bucket, key, options);
    return { ...readHead(response), body: response.body ?? new ReadableStream({ start: (controller) => controller.close() }) };
  }

  /** An object's headers without its body. */
  async headObject(bucket: string, key: string): Promise<StorageObject> {
    const response = await this.read("HEAD", bucket, key, {});
    return { ...readHead(response), body: new Uint8Array() };
  }

  async deleteObject(bucket: string, key: string): Promise<void> {
    await this.empty({ method: "DELETE", path: this.objectPath(bucket, key) });
  }

  /** Lists the objects of a bucket, one page at a time. */
  async listObjects(bucket: string, options: ListOptions = {}): Promise<ListPage> {
    const raw = await this.json({
      method: "GET",
      path: `/v1/buckets/${segment(bucket)}/objects`,
      query: {
        prefix: options.prefix,
        delimiter: options.delimiter,
        max_keys: options.maxKeys,
        continuation_token: options.continuationToken,
        start_after: options.startAfter,
      },
    });
    return {
      objects: raw.objects.map((item: any) => ({
        key: item.key,
        size: item.size,
        etag: item.etag,
        contentType: item.content_type,
        lastModified: item.last_modified,
      })),
      commonPrefixes: raw.common_prefixes,
      ...(raw.next_continuation_token != null && { nextContinuationToken: raw.next_continuation_token }),
    };
  }

  /** Copies an object, in this or another bucket, without moving its bytes. */
  async copyObject(sourceBucket: string, sourceKey: string, bucket: string, key: string, options: CopyOptions = {}): Promise<PutResult> {
    const headers = options.replace ? { ...objectHeaders(options.replace), "x-vgr-metadata-directive": "REPLACE" } : {};
    return this.json({
      method: "PUT",
      path: this.objectPath(bucket, key),
      headers: { ...headers, "x-vgr-copy-source": `${sourceBucket}/${keyPath(sourceKey)}` },
    });
  }

  /** Lists the multipart uploads in progress. */
  async listUploads(bucket: string, options: ListUploadsOptions = {}): Promise<UploadsPage> {
    const raw = await this.json({
      method: "GET",
      path: `/v1/buckets/${segment(bucket)}/uploads`,
      query: {
        prefix: options.prefix,
        delimiter: options.delimiter,
        max_uploads: options.maxUploads,
        key_marker: options.keyMarker,
        upload_id_marker: options.uploadIdMarker,
      },
    });
    return {
      uploads: raw.uploads.map((item: any) => ({ uploadId: item.upload_id, key: item.key, initiated: item.initiated })),
      commonPrefixes: raw.common_prefixes,
      isTruncated: raw.is_truncated,
      ...(raw.next_key_marker != null && { nextKeyMarker: raw.next_key_marker }),
      ...(raw.next_upload_id_marker != null && { nextUploadIdMarker: raw.next_upload_id_marker }),
    };
  }

  /** Starts a multipart upload and returns its id. */
  async createUpload(bucket: string, key: string, options: PutOptions = {}): Promise<string> {
    const raw = await this.json({
      method: "POST",
      path: `/v1/buckets/${segment(bucket)}/uploads`,
      json: compact({ key, content_type: options.contentType, metadata: options.metadata ?? {}, tags: options.tags ?? {} }),
    });
    return raw.upload_id;
  }

  /** Uploads part `number` of a multipart upload in one request. */
  async uploadPart(bucket: string, uploadId: string, number: number, body: Uint8Array): Promise<CompletedPart> {
    return partOf(await this.json({ method: "PUT", path: `/v1/buckets/${segment(bucket)}/uploads/${segment(uploadId)}/parts/${number}`, body }));
  }

  /** The parts stored so far in a multipart upload, by part number. */
  async listParts(bucket: string, uploadId: string): Promise<UploadedPart[]> {
    const raw = await this.json({ method: "GET", path: `/v1/buckets/${segment(bucket)}/uploads/${segment(uploadId)}/parts` });
    return raw.parts.map((part: any) => ({ number: part.number, etag: part.etag, size: part.size }));
  }

  /** Completes a multipart upload from its parts and returns the stored object. */
  completeUpload(bucket: string, uploadId: string, parts: CompletedPart[]): Promise<PutResult> {
    return this.json({
      method: "POST",
      path: `/v1/buckets/${segment(bucket)}/uploads/${segment(uploadId)}/complete`,
      json: { parts: parts.map((part) => ({ number: part.number, etag: part.etag })) },
    });
  }

  /** Aborts a multipart upload and discards its parts. */
  async abortUpload(bucket: string, uploadId: string): Promise<void> {
    await this.empty({ method: "DELETE", path: `/v1/buckets/${segment(bucket)}/uploads/${segment(uploadId)}` });
  }

  /** Uploads a stream of any length in parts, aborting the upload when a part fails. */
  async uploadStream(
    bucket: string,
    key: string,
    body: AsyncIterable<Uint8Array> | ReadableStream<Uint8Array>,
    options: UploadStreamOptions = {},
  ): Promise<PutResult> {
    const partSize = options.partSize ?? DEFAULT_PART_SIZE;
    const uploadId = await this.createUpload(bucket, key, options);
    try {
      const parts: CompletedPart[] = [];
      const store = async (part: Uint8Array) => {
        parts.push(await this.uploadPart(bucket, uploadId, parts.length + 1, part));
      };
      let pending: Uint8Array[] = [];
      let pendingLength = 0;
      for await (const chunk of body as AsyncIterable<Uint8Array>) {
        pending.push(chunk);
        pendingLength += chunk.length;
        if (pendingLength < partSize) continue;
        let joined = concat(pending, pendingLength);
        while (joined.length >= partSize) {
          await store(joined.subarray(0, partSize));
          joined = joined.subarray(partSize);
        }
        pending = joined.length ? [joined] : [];
        pendingLength = joined.length;
      }
      const buffer = concat(pending, pendingLength);
      if (buffer.length > 0 || parts.length === 0) await store(buffer);
      return await this.completeUpload(bucket, uploadId, parts);
    } catch (error) {
      await this.abortUpload(bucket, uploadId).catch(() => undefined);
      throw error;
    }
  }

  /** Replaces the tags of an object. */
  async putTags(bucket: string, key: string, tags: Record<string, string>): Promise<void> {
    await this.empty({ method: "PUT", path: `${this.objectPath(bucket, key)}?tagging`, json: { tags } });
  }

  async getTags(bucket: string, key: string): Promise<Record<string, string>> {
    return (await this.json({ method: "GET", path: `${this.objectPath(bucket, key)}?tagging` })).tags;
  }

  async deleteTags(bucket: string, key: string): Promise<void> {
    await this.empty({ method: "DELETE", path: `${this.objectPath(bucket, key)}?tagging` });
  }

  /** Creates a presigned URL for downloading or uploading an object without credentials. */
  async presign(bucket: string, request: PresignRequest): Promise<Presigned> {
    const raw = await this.json({
      method: "POST",
      path: `/v1/buckets/${segment(bucket)}/presign`,
      json: compact({
        key: request.key,
        method: request.method,
        expires_in: request.expiresIn,
        content_type: request.contentType,
        max_size: request.maxSize,
        response_content_disposition: request.responseContentDisposition,
        checksum_sha256: request.checksumSha256,
        upload_id: request.uploadId,
        part_number: request.partNumber,
      }),
    });
    return { url: raw.url, expiresAt: raw.expires_at };
  }

  /** Replaces the lifecycle rules of a bucket. */
  async putLifecycle(bucket: string, rules: LifecycleRule[]): Promise<void> {
    await this.empty({
      method: "PUT",
      path: `/v1/buckets/${segment(bucket)}/lifecycle`,
      json: {
        rules: rules.map((rule) =>
          compact({
            id: rule.id,
            prefix: rule.prefix,
            tags: rule.tags && Object.keys(rule.tags).length ? rule.tags : undefined,
            expire_after_days: rule.expireAfterDays,
            abort_incomplete_uploads_after_days: rule.abortIncompleteUploadsAfterDays,
            enabled: rule.enabled,
          }),
        ),
      },
    });
  }

  async getLifecycle(bucket: string): Promise<LifecycleRule[]> {
    return (await this.json({ method: "GET", path: `/v1/buckets/${segment(bucket)}/lifecycle` })).rules.map(ruleOf);
  }

  async deleteLifecycle(bucket: string): Promise<void> {
    await this.empty({ method: "DELETE", path: `/v1/buckets/${segment(bucket)}/lifecycle` });
  }
}

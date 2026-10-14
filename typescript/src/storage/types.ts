export interface Bucket {
  name: string;
  projectId: string;
  /** Creation time, RFC 3339. */
  createdAt: string;
}

export interface PutOptions {
  /** Content type stored with the object. */
  contentType?: string;
  /** User metadata, sent as `x-vgr-meta-<name>` headers. */
  metadata?: Record<string, string>;
  tags?: Record<string, string>;
}

export interface PutResult {
  etag: string;
  size: number;
}

export interface ObjectHead {
  contentType?: string;
  etag?: string;
  /** Size of the whole object, also for a range. */
  size?: number;
  metadata: Record<string, string>;
  /** Lowercase hex SHA-256 of the object, once the service knows it. */
  checksumSha256?: string;
}

/** A downloaded object or byte range. */
export interface StorageObject extends ObjectHead {
  body: Uint8Array;
}

/** A download whose body is read as it arrives. */
export interface StorageObjectStream extends ObjectHead {
  body: ReadableStream<Uint8Array>;
}

export interface ReadOptions {
  /** The inclusive byte range `[first, last]`. */
  range?: [first: number, last: number];
}

export interface ListOptions {
  prefix?: string;
  /** Groups keys that share a prefix up to the delimiter into common prefixes. */
  delimiter?: string;
  maxKeys?: number;
  continuationToken?: string;
  /** A key after which the listing starts; ignored with a continuation token. */
  startAfter?: string;
}

export interface ObjectSummary {
  key: string;
  size: number;
  etag: string;
  contentType: string;
  /** Last modification time, RFC 3339. */
  lastModified: string;
}

export interface ListPage {
  objects: ObjectSummary[];
  commonPrefixes: string[];
  /** Token of the next page; undefined on the last page. */
  nextContinuationToken?: string;
}

export interface CopyOptions {
  /** The new object takes its content type, metadata and tags from these options; without it the source's are kept. */
  replace?: PutOptions;
}

export interface ListUploadsOptions {
  prefix?: string;
  delimiter?: string;
  maxUploads?: number;
  keyMarker?: string;
  uploadIdMarker?: string;
}

export interface UploadSummary {
  uploadId: string;
  key: string;
  /** Start time, RFC 3339. */
  initiated: string;
}

export interface UploadsPage {
  uploads: UploadSummary[];
  commonPrefixes: string[];
  isTruncated: boolean;
  nextKeyMarker?: string;
  nextUploadIdMarker?: string;
}

export interface UploadedPart {
  number: number;
  etag: string;
  size: number;
}

export interface CompletedPart {
  number: number;
  etag: string;
}

export interface UploadStreamOptions extends PutOptions {
  /** Bytes per part; 5 MiB by default, the smallest part the service accepts. */
  partSize?: number;
}

export interface PresignRequest {
  key: string;
  /** `GET` or `PUT`. */
  method: "GET" | "PUT";
  /** Validity in seconds. */
  expiresIn: number;
  /** Content type an upload must declare. */
  contentType?: string;
  /** Largest size in bytes an upload may have. */
  maxSize?: number;
  /** The `Content-Disposition` a download's response states; part of the signature. */
  responseContentDisposition?: string;
  /** Lowercase hex SHA-256 an upload's content must have. */
  checksumSha256?: string;
  /** With `partNumber`, a `PUT` stores that part of the multipart upload. */
  uploadId?: string;
  partNumber?: number;
}

export interface Presigned {
  url: string;
  /** Expiry time, RFC 3339. */
  expiresAt: string;
}

export interface LifecycleRule {
  /** Name of the rule, unique within the bucket. */
  id: string;
  /** Applies to keys that start with this prefix. */
  prefix?: string;
  /** Applies to objects that carry all these tags. */
  tags?: Record<string, string>;
  /** Deletes objects this many days after creation. */
  expireAfterDays?: number;
  /** Aborts multipart uploads still incomplete after this many days. */
  abortIncompleteUploadsAfterDays?: number;
  /** A disabled rule is stored and read back but never applied; true by default. */
  enabled?: boolean;
}

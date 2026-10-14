import { createHash } from "node:crypto";
import { afterEach, describe, expect, it } from "vitest";

import { StorageClient, StorageError } from "../../src/storage/index.js";
import { fakeServer, jsonReply, type FakeServer } from "../helpers/fake-server.js";

const credentials = { accessKeyId: "VGAKTEST", secretAccessKey: "secret" };

const servers: FakeServer[] = [];
afterEach(async () => {
  while (servers.length) await servers.pop()!.close();
});

async function storage(respond: Parameters<typeof fakeServer>[0]) {
  const server = await fakeServer(respond);
  servers.push(server);
  return { server, client: new StorageClient({ endpoint: server.url, credentials }) };
}

const sha256 = (data: string | Uint8Array) => createHash("sha256").update(data).digest("hex");
const bucket = { name: "photos", project_id: "p1", created_at: "2026-10-01T12:00:00Z" };

describe("StorageClient buckets", () => {
  it("creates, lists, reads and deletes buckets", async () => {
    const { server, client } = await storage((request) => {
      if (request.method === "DELETE") return { status: 204 };
      if (request.method === "GET" && request.target === "/v1/buckets") return jsonReply({ buckets: [bucket] });
      return jsonReply(bucket, request.method === "POST" ? 201 : 200);
    });
    expect(await client.createBucket("photos")).toEqual({ name: "photos", projectId: "p1", createdAt: "2026-10-01T12:00:00Z" });
    expect([server.last().method, server.last().target]).toEqual(["POST", "/v1/buckets"]);
    expect(server.last().json()).toEqual({ name: "photos" });
    expect(await client.listBuckets()).toHaveLength(1);
    expect((await client.getBucket("photos")).name).toBe("photos");
    expect(server.last().target).toBe("/v1/buckets/photos");
    await client.deleteBucket("photos");
    expect([server.last().method, server.last().target]).toEqual(["DELETE", "/v1/buckets/photos"]);
  });

  it("signs every call VGR4 for storage", async () => {
    const { server, client } = await storage(() => jsonReply({ buckets: [] }));
    await client.listBuckets();
    expect(server.last().headers.authorization).toMatch(/^VGR4-HMAC-SHA256 Credential=VGAKTEST\/\d{8}\/storage\/vgr4_request, /);
  });

  it("raises StorageError with the status, code and message", async () => {
    const { client } = await storage(() => jsonReply({ error: { code: "NoSuchBucket", message: "gone" } }, 404));
    const error = await client.getBucket("nope").catch((e) => e);
    expect(error).toBeInstanceOf(StorageError);
    expect(error).toMatchObject({ status: 404, code: "NoSuchBucket", message: "gone" });
  });

  it("keeps the status and body of an answer that is not an error envelope", async () => {
    const { client } = await storage(() => ({ status: 502, body: "bad gateway" }));
    const error = await client.getBucket("nope").catch((e) => e);
    expect(error).toMatchObject({ status: 502, code: "UnknownError", message: "bad gateway" });
  });
});

describe("StorageClient objects", () => {
  it("uploads with a signed payload, metadata and tags", async () => {
    const { server, client } = await storage(() => jsonReply({ etag: "e1", size: 5 }));
    const result = await client.putObject("photos", "2026/cat photo.jpg", new TextEncoder().encode("hello"), {
      contentType: "image/jpeg",
      metadata: { owner: "an" },
      tags: { kind: "pet", "a b": "c&d" },
    });
    expect(result).toEqual({ etag: "e1", size: 5 });
    const seen = server.last();
    expect([seen.method, seen.target]).toEqual(["PUT", "/v1/buckets/photos/objects/2026/cat%20photo.jpg"]);
    expect(seen.headers["content-type"]).toBe("image/jpeg");
    expect(seen.headers["x-vgr-meta-owner"]).toBe("an");
    expect(seen.headers["x-vgr-tagging"]).toBe("a%20b=c%26d&kind=pet");
    expect(seen.headers["x-vgr-content-sha256"]).toBe(sha256("hello"));
    expect(seen.text()).toBe("hello");
  });

  it("streams an upload of known length as UNSIGNED-PAYLOAD", async () => {
    const { server, client } = await storage(() => jsonReply({ etag: "e2", size: 6 }));
    const chunks = [new TextEncoder().encode("abc"), new TextEncoder().encode("def")];
    const stream = new ReadableStream<Uint8Array>({
      pull(controller) {
        const chunk = chunks.shift();
        if (chunk) controller.enqueue(chunk);
        else controller.close();
      },
    });
    const result = await client.putObjectStream("photos", "big.bin", stream, 6, { contentType: "application/octet-stream" });
    expect(result.size).toBe(6);
    const seen = server.last();
    expect(seen.headers["x-vgr-content-sha256"]).toBe("UNSIGNED-PAYLOAD");
    expect(seen.headers["content-length"]).toBe("6");
    expect(seen.text()).toBe("abcdef");
  });

  it("streams a Blob as UNSIGNED-PAYLOAD", async () => {
    const { server, client } = await storage(() => jsonReply({ etag: "e2", size: 3 }));
    await client.putObjectStream("photos", "b.bin", new Blob(["xyz"]), 3);
    expect(server.last().headers["x-vgr-content-sha256"]).toBe("UNSIGNED-PAYLOAD");
    expect(server.last().text()).toBe("xyz");
  });

  it("downloads an object, or a range of it", async () => {
    const { server, client } = await storage(() => ({
      status: 206,
      headers: { "content-type": "text/plain", etag: '"abc"', "content-range": "bytes 0-1/9", "x-vgr-meta-owner": "an", "x-vgr-checksum-sha256": "ff" },
      body: "he",
    }));
    const object = await client.getObject("photos", "a.txt", { range: [0, 1] });
    expect(server.last().headers.range).toBe("bytes=0-1");
    expect(Buffer.from(object.body).toString()).toBe("he");
    expect(object).toMatchObject({ contentType: "text/plain", etag: "abc", size: 9, metadata: { owner: "an" }, checksumSha256: "ff" });
  });

  it("streams a download", async () => {
    const { client } = await storage(() => ({ headers: { "content-type": "text/plain", "content-length": "6" }, body: ["abc", "def"] }));
    const object = await client.getObjectStream("photos", "a.txt");
    expect(object.size).toBe(6);
    const parts: Uint8Array[] = [];
    for await (const chunk of object.body as unknown as AsyncIterable<Uint8Array>) parts.push(chunk);
    expect(Buffer.concat(parts).toString()).toBe("abcdef");
  });

  it("reads the headers of an object with HEAD", async () => {
    const { server, client } = await storage(() => ({ headers: { "content-type": "image/png", "content-length": "12", etag: '"e"' } }));
    const object = await client.headObject("photos", "a.png");
    expect(server.last().method).toBe("HEAD");
    expect(object).toMatchObject({ contentType: "image/png", size: 12, etag: "e" });
    expect(object.body.byteLength).toBe(0);
  });

  it("deletes an object", async () => {
    const { server, client } = await storage(() => ({ status: 204 }));
    await client.deleteObject("photos", "a/b.txt");
    expect([server.last().method, server.last().target]).toEqual(["DELETE", "/v1/buckets/photos/objects/a/b.txt"]);
  });

  it("lists objects with filters", async () => {
    const page = { objects: [{ key: "a", size: 1, etag: "e", content_type: "text/plain", last_modified: "2026-10-01T12:00:00Z" }], common_prefixes: ["x/"], next_continuation_token: "t2" };
    const { server, client } = await storage(() => jsonReply(page));
    const listed = await client.listObjects("photos", { prefix: "a/", delimiter: "/", maxKeys: 10, continuationToken: "t1", startAfter: "a/0" });
    expect(server.last().target).toBe("/v1/buckets/photos/objects?prefix=a%2F&delimiter=%2F&max_keys=10&continuation_token=t1&start_after=a%2F0");
    expect(listed).toEqual({
      objects: [{ key: "a", size: 1, etag: "e", contentType: "text/plain", lastModified: "2026-10-01T12:00:00Z" }],
      commonPrefixes: ["x/"],
      nextContinuationToken: "t2",
    });
    await client.listObjects("photos");
    expect(server.last().target).toBe("/v1/buckets/photos/objects");
  });

  it("copies an object, keeping or replacing its description", async () => {
    const { server, client } = await storage(() => jsonReply({ etag: "e3", size: 4 }));
    await client.copyObject("src", "a b.txt", "dst", "c.txt");
    let seen = server.last();
    expect([seen.method, seen.target]).toEqual(["PUT", "/v1/buckets/dst/objects/c.txt"]);
    expect(seen.headers["x-vgr-copy-source"]).toBe("src/a%20b.txt");
    expect(seen.headers["x-vgr-metadata-directive"]).toBeUndefined();
    await client.copyObject("src", "a.txt", "dst", "c.txt", { replace: { contentType: "text/html", tags: { k: "v" } } });
    seen = server.last();
    expect(seen.headers["x-vgr-metadata-directive"]).toBe("REPLACE");
    expect(seen.headers["content-type"]).toBe("text/html");
    expect(seen.headers["x-vgr-tagging"]).toBe("k=v");
  });
});

describe("StorageClient multipart uploads", () => {
  it("covers the multipart calls", async () => {
    const { server, client } = await storage((request) => {
      if (request.method === "POST" && request.target === "/v1/buckets/photos/uploads") return jsonReply({ upload_id: "u1" }, 201);
      if (request.method === "PUT") return jsonReply({ number: 1, etag: "p1" });
      if (request.method === "GET" && request.target.endsWith("/parts")) return jsonReply({ parts: [{ number: 1, etag: "p1", size: 3 }] });
      if (request.method === "GET") {
        return jsonReply({ uploads: [{ upload_id: "u1", key: "k", initiated: "2026-10-01T12:00:00Z" }], common_prefixes: [], is_truncated: true, next_key_marker: "k", next_upload_id_marker: "u1" });
      }
      if (request.method === "DELETE") return { status: 204 };
      return jsonReply({ etag: "done", size: 3 });
    });
    expect(await client.createUpload("photos", "big.bin", { contentType: "video/mp4", metadata: { a: "b" }, tags: { t: "1" } })).toBe("u1");
    expect(server.last().json()).toEqual({ key: "big.bin", content_type: "video/mp4", metadata: { a: "b" }, tags: { t: "1" } });
    expect(await client.uploadPart("photos", "u1", 1, new TextEncoder().encode("abc"))).toEqual({ number: 1, etag: "p1" });
    expect([server.last().method, server.last().target]).toEqual(["PUT", "/v1/buckets/photos/uploads/u1/parts/1"]);
    expect(await client.listParts("photos", "u1")).toEqual([{ number: 1, etag: "p1", size: 3 }]);
    expect(await client.completeUpload("photos", "u1", [{ number: 1, etag: "p1" }])).toEqual({ etag: "done", size: 3 });
    expect(server.last().json()).toEqual({ parts: [{ number: 1, etag: "p1" }] });
    const uploads = await client.listUploads("photos", { prefix: "k", maxUploads: 5, keyMarker: "a", uploadIdMarker: "b" });
    expect(server.last().target).toBe("/v1/buckets/photos/uploads?prefix=k&max_uploads=5&key_marker=a&upload_id_marker=b");
    expect(uploads).toMatchObject({ isTruncated: true, nextKeyMarker: "k", nextUploadIdMarker: "u1", uploads: [{ uploadId: "u1", key: "k" }] });
    await client.abortUpload("photos", "u1");
    expect([server.last().method, server.last().target]).toEqual(["DELETE", "/v1/buckets/photos/uploads/u1"]);
  });

  it("uploads a stream in parts and completes", async () => {
    const parts: string[] = [];
    const { server, client } = await storage((request) => {
      if (request.method === "POST" && request.target.endsWith("/uploads")) return jsonReply({ upload_id: "u1" }, 201);
      if (request.method === "PUT") {
        parts.push(request.text());
        return jsonReply({ number: parts.length, etag: `p${parts.length}` });
      }
      return jsonReply({ etag: "done", size: 10 });
    });
    async function* source() {
      for (const piece of ["abc", "defg", "hi", "j"]) yield new TextEncoder().encode(piece);
    }
    const result = await client.uploadStream("photos", "big.bin", source(), { partSize: 4 });
    expect(result).toEqual({ etag: "done", size: 10 });
    expect(parts).toEqual(["abcd", "efgh", "ij"]);
    expect(server.last().json()).toEqual({ parts: [{ number: 1, etag: "p1" }, { number: 2, etag: "p2" }, { number: 3, etag: "p3" }] });
  });

  it("uploads an empty stream as one empty part", async () => {
    const parts: string[] = [];
    const { client } = await storage((request) => {
      if (request.method === "POST" && request.target.endsWith("/uploads")) return jsonReply({ upload_id: "u1" }, 201);
      if (request.method === "PUT") {
        parts.push(request.text());
        return jsonReply({ number: 1, etag: "p1" });
      }
      return jsonReply({ etag: "done", size: 0 });
    });
    async function* nothing() {}
    await client.uploadStream("photos", "empty", nothing());
    expect(parts).toEqual([""]);
  });

  it("aborts the upload when a part fails", async () => {
    let puts = 0;
    const { server, client } = await storage((request) => {
      if (request.method === "POST" && request.target.endsWith("/uploads")) return jsonReply({ upload_id: "u1" }, 201);
      if (request.method === "PUT") {
        puts += 1;
        return puts === 2 ? jsonReply({ error: { code: "InternalError", message: "disk" } }, 500) : jsonReply({ number: puts, etag: `p${puts}` });
      }
      return { status: 204 };
    });
    async function* source() {
      yield new TextEncoder().encode("aaaabbbbcccc");
    }
    const error = await client.uploadStream("photos", "big.bin", source(), { partSize: 4 }).catch((e) => e);
    expect(error).toMatchObject({ status: 500, code: "InternalError" });
    expect([server.last().method, server.last().target]).toEqual(["DELETE", "/v1/buckets/photos/uploads/u1"]);
  });
});

describe("StorageClient tags, lifecycle and presigned URLs", () => {
  it("puts, reads and deletes tags", async () => {
    const { server, client } = await storage((request) =>
      request.method === "GET" ? jsonReply({ tags: { my_tag: "v" } }) : { status: 204 },
    );
    await client.putTags("photos", "a.txt", { k: "v" });
    expect([server.last().method, server.last().target]).toEqual(["PUT", "/v1/buckets/photos/objects/a.txt?tagging"]);
    expect(server.last().json()).toEqual({ tags: { k: "v" } });
    expect(await client.getTags("photos", "a.txt")).toEqual({ my_tag: "v" });
    await client.deleteTags("photos", "a.txt");
    expect(server.last().method).toBe("DELETE");
  });

  it("puts, reads and deletes lifecycle rules", async () => {
    const { server, client } = await storage((request) =>
      request.method === "GET"
        ? jsonReply({ rules: [{ id: "r", prefix: "tmp/", expire_after_days: 3, enabled: true }] })
        : { status: 204 },
    );
    await client.putLifecycle("photos", [{ id: "r", prefix: "tmp/", expireAfterDays: 3, abortIncompleteUploadsAfterDays: 1, tags: { t: "1" }, enabled: false }]);
    expect(server.last().json()).toEqual({
      rules: [{ id: "r", prefix: "tmp/", tags: { t: "1" }, expire_after_days: 3, abort_incomplete_uploads_after_days: 1, enabled: false }],
    });
    expect(await client.getLifecycle("photos")).toEqual([{ id: "r", prefix: "tmp/", tags: {}, expireAfterDays: 3, enabled: true }]);
    await client.deleteLifecycle("photos");
    expect([server.last().method, server.last().target]).toEqual(["DELETE", "/v1/buckets/photos/lifecycle"]);
  });

  it("presigns a URL", async () => {
    const { server, client } = await storage(() => jsonReply({ url: "https://s/x?sig=1", expires_at: "2026-10-01T13:00:00Z" }));
    const presigned = await client.presign("photos", {
      key: "a.txt", method: "PUT", expiresIn: 600, contentType: "text/plain", maxSize: 100, checksumSha256: "ab", uploadId: "u1", partNumber: 2,
    });
    expect(presigned).toEqual({ url: "https://s/x?sig=1", expiresAt: "2026-10-01T13:00:00Z" });
    expect([server.last().method, server.last().target]).toEqual(["POST", "/v1/buckets/photos/presign"]);
    expect(server.last().json()).toEqual({
      key: "a.txt", method: "PUT", expires_in: 600, content_type: "text/plain", max_size: 100, checksum_sha256: "ab", upload_id: "u1", part_number: 2,
    });
    await client.presign("photos", { key: "a.txt", method: "GET", expiresIn: 60, responseContentDisposition: "attachment" });
    expect(server.last().json()).toEqual({ key: "a.txt", method: "GET", expires_in: 60, response_content_disposition: "attachment" });
  });
});

describe("StorageClient credentials", () => {
  it("invalidates the provider when the service refuses the credentials", async () => {
    let invalidated = 0;
    const server = await fakeServer(() => jsonReply({ error: { code: "AccessDenied", message: "no" } }, 403));
    servers.push(server);
    const provider = Object.assign(() => credentials, { invalidate: () => void (invalidated += 1) });
    const client = new StorageClient({ endpoint: server.url, credentials: provider });
    await expect(client.listBuckets()).rejects.toBeInstanceOf(StorageError);
    expect(invalidated).toBe(1);
  });
});

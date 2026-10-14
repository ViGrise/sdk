import { createHash } from "node:crypto";

export interface MultipartPart {
  name: string;
  body: Uint8Array;
  fileName?: string;
  contentType?: string;
}

const encoder = new TextEncoder();

function contains(haystack: Uint8Array, needle: Uint8Array): boolean {
  return Buffer.from(haystack.buffer, haystack.byteOffset, haystack.byteLength).includes(Buffer.from(needle));
}

/** Encodes a `multipart/form-data` body. The boundary is derived from the parts, so it cannot occur inside them. */
export function encodeMultipart(parts: MultipartPart[]): { contentType: string; body: Uint8Array } {
  let boundary = "";
  for (let salt = 0; ; salt += 1) {
    const digest = createHash("sha256").update(String(salt));
    for (const part of parts) digest.update(part.body);
    boundary = `vigrise-${digest.digest("hex").slice(0, 32)}`;
    const marker = encoder.encode(boundary);
    if (!parts.some((part) => contains(part.body, marker))) break;
  }
  const chunks: Uint8Array[] = [];
  for (const part of parts) {
    let head = `--${boundary}\r\nContent-Disposition: form-data; name="${part.name}"`;
    if (part.fileName !== undefined) head += `; filename="${part.fileName.replaceAll('"', "%22")}"\r\nContent-Type: ${part.contentType}`;
    chunks.push(encoder.encode(`${head}\r\n\r\n`), part.body, encoder.encode("\r\n"));
  }
  chunks.push(encoder.encode(`--${boundary}--\r\n`));
  return { contentType: `multipart/form-data; boundary=${boundary}`, body: new Uint8Array(Buffer.concat(chunks)) };
}

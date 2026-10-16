export interface MultipartPart {
  name: string;
  body: Uint8Array;
  fileName?: string;
  contentType?: string;
}

const encoder = new TextEncoder();

function contains(haystack: Uint8Array, needle: Uint8Array): boolean {
  outer: for (let start = 0; start + needle.length <= haystack.length; start += 1) {
    for (let offset = 0; offset < needle.length; offset += 1) {
      if (haystack[start + offset] !== needle[offset]) continue outer;
    }
    return true;
  }
  return false;
}

function randomBoundary(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return `vigrise-${[...bytes].map((byte) => byte.toString(16).padStart(2, "0")).join("")}`;
}

/** Encodes a `multipart/form-data` body with a random boundary that occurs in none of the parts. */
export function encodeMultipart(parts: MultipartPart[]): { contentType: string; body: Uint8Array } {
  for (const part of parts) {
    if (part.contentType !== undefined && /[\r\n]/.test(part.contentType))
      throw new Error("a content type cannot contain line breaks");
  }
  let boundary = randomBoundary();
  while (parts.some((part) => contains(part.body, encoder.encode(boundary)))) boundary = randomBoundary();
  const chunks: Uint8Array[] = [];
  for (const part of parts) {
    let head = `--${boundary}\r\nContent-Disposition: form-data; name="${part.name}"`;
    if (part.fileName !== undefined) {
      const fileName = part.fileName.replaceAll('"', "%22").replaceAll("\r", "%0D").replaceAll("\n", "%0A");
      head += `; filename="${fileName}"\r\nContent-Type: ${part.contentType}`;
    }
    chunks.push(encoder.encode(`${head}\r\n\r\n`), part.body, encoder.encode("\r\n"));
  }
  chunks.push(encoder.encode(`--${boundary}--\r\n`));
  const body = new Uint8Array(chunks.reduce((total, chunk) => total + chunk.length, 0));
  let offset = 0;
  for (const chunk of chunks) {
    body.set(chunk, offset);
    offset += chunk.length;
  }
  return { contentType: `multipart/form-data; boundary=${boundary}`, body };
}

// Canonical request construction. Must stay byte-for-byte identical to
// test-vectors/generate.py and the Rust crate.

const UNRESERVED = /^[A-Za-z0-9\-_.~]$/;
const HEX = /^[0-9A-Fa-f]{2}$/;

export type HeaderList = [string, string][];

/** Percent-decodes to raw bytes; malformed escapes stay literal and never throw. */
function decodeToBytes(raw: string): number[] {
  const bytes: number[] = [];
  const encoder = new TextEncoder();
  for (let i = 0; i < raw.length; ) {
    const pair = raw.slice(i + 1, i + 3);
    if (raw[i] === "%" && HEX.test(pair)) {
      bytes.push(parseInt(pair, 16));
      i += 3;
      continue;
    }
    const codePoint = raw.codePointAt(i)!;
    const char = String.fromCodePoint(codePoint);
    bytes.push(...encoder.encode(char));
    i += char.length;
  }
  return bytes;
}

/** Decodes then encodes every byte outside the RFC 3986 unreserved set; `+` is a literal. */
function reencode(raw: string): string {
  return decodeToBytes(raw)
    .map((b) => {
      const c = String.fromCharCode(b);
      return b < 0x80 && UNRESERVED.test(c) ? c : `%${b.toString(16).toUpperCase().padStart(2, "0")}`;
    })
    .join("");
}

export function canonicalUri(rawPath: string): string {
  return rawPath === "" ? "/" : rawPath.split("/").map(reencode).join("/");
}

export function canonicalQuery(rawQuery: string): string {
  const pairs = rawQuery
    .split("&")
    .filter((part) => part !== "")
    .map((part) => {
      const at = part.indexOf("=");
      const [key, value] = at === -1 ? [part, ""] : [part.slice(0, at), part.slice(at + 1)];
      return [reencode(key), reencode(value)] as const;
    });
  pairs.sort(([ka, va], [kb, vb]) => (ka < kb ? -1 : ka > kb ? 1 : va < vb ? -1 : va > vb ? 1 : 0));
  return pairs.map(([k, v]) => `${k}=${v}`).join("&");
}

/** One `name:value\n` line per signed header; repeated values join with `,`. */
export function canonicalHeaders(headers: HeaderList, signedHeaders: string[]): string {
  return signedHeaders
    .map((name) => {
      const values = headers
        .filter(([n]) => n.toLowerCase() === name)
        .map(([, v]) => v.split(/\s+/).filter(Boolean).join(" "));
      if (values.length === 0) {
        throw new Error(`Signed header ${name} is missing from the request`);
      }
      return `${name}:${values.join(",")}\n`;
    })
    .join("");
}

export function canonicalRequest(
  method: string,
  rawPath: string,
  rawQuery: string,
  headers: HeaderList,
  signedHeaders: string[],
  payloadHash: string,
): string {
  return [
    method.toUpperCase(),
    canonicalUri(rawPath),
    canonicalQuery(rawQuery),
    canonicalHeaders(headers, signedHeaders),
    signedHeaders.join(";"),
    payloadHash,
  ].join("\n");
}

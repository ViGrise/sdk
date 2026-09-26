import { createHash } from "node:crypto";

export * from "./canonical-core.js";

export function sha256Hex(data: Uint8Array | string): string {
  return createHash("sha256").update(data).digest("hex");
}

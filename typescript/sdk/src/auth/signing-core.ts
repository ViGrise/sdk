export const ALGORITHM = "VGR4-HMAC-SHA256";
export const TERMINATOR = "vgr4_request";

export function scope(date: string, service: string): string {
  return `${date}/${service}/${TERMINATOR}`;
}

/** `YYYYMMDDTHHMMSSZ` in UTC. */
export function formatTimestamp(date: Date): string {
  return date
    .toISOString()
    .replace(/[-:]/g, "")
    .replace(/\.\d{3}Z$/, "Z");
}

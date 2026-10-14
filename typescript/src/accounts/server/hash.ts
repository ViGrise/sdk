import { createHash } from "node:crypto";

/** Request header that carries the `vigrise_sso` cookie value from an app's frontend to its backend. */
export const SSO_HEADER = "x-vigrise-sso";

/** Hash of a `vigrise_sso` cookie value: lowercase hex SHA-256. Apps store it to notice when the cookie changes. */
export function ssoTokenHash(token: string): string {
  return createHash("sha256").update(token).digest("hex");
}

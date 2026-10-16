/** Lowercase SHA-256 hex digest of a `vigrise_sso` cookie value, as Accounts stores it. */
export async function ssoTokenHash(token: string): Promise<string> {
  const digest = await globalThis.crypto.subtle.digest("SHA-256", new TextEncoder().encode(token));
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("");
}

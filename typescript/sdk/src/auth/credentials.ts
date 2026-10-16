import { readFileSync } from "node:fs";
import { join } from "node:path";

import type { Credentials } from "./types.js";

export type { Credentials } from "./types.js";

/** Reads one profile of the INI-style shared credentials file. */
export function parseProfile(contents: string, profile: string): Credentials | undefined {
  // Editors on Windows commonly prefix UTF-8 files with a byte order mark.
  const text = contents.startsWith("﻿") ? contents.slice(1) : contents;
  let inProfile = false;
  let accessKeyId: string | undefined;
  let secretAccessKey: string | undefined;
  let sessionToken: string | undefined;
  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (line === "" || line.startsWith("#") || line.startsWith(";")) continue;
    if (line.startsWith("[") && line.endsWith("]")) {
      inProfile = line.slice(1, -1).trim() === profile;
      continue;
    }
    if (!inProfile) continue;
    const at = line.indexOf("=");
    if (at === -1) continue;
    const key = line.slice(0, at).trim();
    const value = line.slice(at + 1).trim();
    if (key === "vigrise_access_key_id") accessKeyId = value;
    if (key === "vigrise_secret_access_key") secretAccessKey = value;
    if (key === "vigrise_session_token") sessionToken = value;
  }
  return accessKeyId && secretAccessKey ? { accessKeyId, secretAccessKey, sessionToken } : undefined;
}

export interface ResolveOptions {
  explicit?: Credentials;
  env?: Record<string, string | undefined>;
}

/**
 * Resolution order: explicit credentials, `VIGRISE_ACCESS_KEY_ID` +
 * `VIGRISE_SECRET_ACCESS_KEY`, then profile `VIGRISE_PROFILE` (default
 * `default`) of `VIGRISE_SHARED_CREDENTIALS_FILE` (default
 * `.vigrise/credentials` under `HOME`, or `USERPROFILE` on Windows).
 */
export function resolveCredentials(options: ResolveOptions = {}): Credentials {
  if (options.explicit) return options.explicit;
  const env = options.env ?? process.env;
  if (env.VIGRISE_ACCESS_KEY_ID && env.VIGRISE_SECRET_ACCESS_KEY) {
    return {
      accessKeyId: env.VIGRISE_ACCESS_KEY_ID,
      secretAccessKey: env.VIGRISE_SECRET_ACCESS_KEY,
      sessionToken: env.VIGRISE_SESSION_TOKEN,
    };
  }
  const home = env.HOME ?? env.USERPROFILE;
  const path = env.VIGRISE_SHARED_CREDENTIALS_FILE ?? (home ? join(home, ".vigrise", "credentials") : undefined);
  const profile = env.VIGRISE_PROFILE ?? "default";
  let contents: string | undefined;
  try {
    contents = path ? readFileSync(path, "utf8") : undefined;
  } catch {
    contents = undefined;
  }
  const found = contents === undefined ? undefined : parseProfile(contents, profile);
  if (!found) {
    throw new Error(
      `No ViGrise credentials found in the environment or ${path ?? "~/.vigrise/credentials"} [${profile}]`,
    );
  }
  return found;
}

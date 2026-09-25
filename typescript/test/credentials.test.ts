import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

import { parseProfile, resolveCredentials } from "../src/credentials.js";

const FILE = "# c\n[default]\nvigrise_access_key_id = VGAKDEFAULT\nvigrise_secret_access_key = s1\n\n[ci]\nvigrise_access_key_id=VGAKCI\nvigrise_secret_access_key=s2\n";

describe("credentials", () => {
  it("parses profiles, CRLF files and a UTF-8 BOM", () => {
    expect(parseProfile(FILE, "ci")?.accessKeyId).toBe("VGAKCI");
    expect(parseProfile(FILE.replaceAll("\n", "\r\n"), "default")?.secretAccessKey).toBe("s1");
    expect(parseProfile("﻿[default]\r\nvigrise_access_key_id = B\r\nvigrise_secret_access_key = s\r\n", "default")?.accessKeyId).toBe("B");
    expect(parseProfile(FILE, "missing")).toBeUndefined();
  });

  it("prefers explicit, then environment, then the shared file", () => {
    expect(resolveCredentials({ explicit: { accessKeyId: "X", secretAccessKey: "x" }, env: {} }).accessKeyId).toBe("X");
    const env = { VIGRISE_ACCESS_KEY_ID: "E", VIGRISE_SECRET_ACCESS_KEY: "e" };
    expect(resolveCredentials({ env }).accessKeyId).toBe("E");
    const home = mkdtempSync(join(tmpdir(), "vigrise-"));
    mkdirSync(join(home, ".vigrise"));
    writeFileSync(join(home, ".vigrise", "credentials"), FILE);
    expect(resolveCredentials({ env: { USERPROFILE: home, VIGRISE_PROFILE: "ci" } }).accessKeyId).toBe("VGAKCI");
    expect(() => resolveCredentials({ env: { HOME: join(home, "none") } })).toThrow(/No ViGrise credentials/);
  });
});

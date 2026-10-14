import { describe, expect, it } from "vitest";

import { envKey, resolveEndpoint, stsBaseUrl, stsUrl } from "../../src/auth/endpoint.js";

describe("resolveEndpoint", () => {
  const env = {
    VIGRISE_ENDPOINT_URL_SPEECH: "http://speech.env",
    VIGRISE_ENDPOINT_URL: "http://all.env",
  };

  it("prefers the explicit endpoint, then the service variable, then the shared one", () => {
    expect(resolveEndpoint("speech", "http://explicit/", env)).toBe("http://explicit");
    expect(resolveEndpoint("speech", undefined, env)).toBe("http://speech.env");
    expect(resolveEndpoint("storage", undefined, env)).toBe("http://all.env");
  });

  it("defaults to the service's API host", () => {
    expect(resolveEndpoint("speech", undefined, {})).toBe("https://speech-api.vigrise.com");
    expect(resolveEndpoint("storage", undefined, {})).toBe("https://storage-api.vigrise.com");
  });

  it("ignores empty values", () => {
    expect(resolveEndpoint("speech", "", { VIGRISE_ENDPOINT_URL_SPEECH: "", VIGRISE_ENDPOINT_URL: "" })).toBe("https://speech-api.vigrise.com");
  });

  it("builds the variable name from the service name", () => {
    expect(envKey("speech")).toBe("VIGRISE_ENDPOINT_URL_SPEECH");
    expect(envKey("voice-studio")).toBe("VIGRISE_ENDPOINT_URL_VOICE_STUDIO");
  });

  it("reads the process environment when none is given", () => {
    process.env.VIGRISE_ENDPOINT_URL_SPEECH = "http://from-process";
    try {
      expect(resolveEndpoint("speech")).toBe("http://from-process");
    } finally {
      delete process.env.VIGRISE_ENDPOINT_URL_SPEECH;
    }
  });
});

describe("stsUrl", () => {
  it("ends in the assume-role path, whichever way the endpoint is given", () => {
    expect(stsUrl(undefined, {})).toBe("https://sts-api.vigrise.com/v1/assume-role");
    expect(stsUrl(undefined, { VIGRISE_ENDPOINT_URL: "http://local:8080" })).toBe("http://local:8080/v1/assume-role");
    expect(stsUrl(undefined, { VIGRISE_ENDPOINT_URL_STS: "http://sts.internal/v1/assume-role" })).toBe("http://sts.internal/v1/assume-role");
    expect(stsUrl("http://explicit", {})).toBe("http://explicit/v1/assume-role");
  });

  it("has a base form without the path", () => {
    expect(stsBaseUrl(undefined, { VIGRISE_ENDPOINT_URL_STS: "http://sts.internal/v1/assume-role" })).toBe("http://sts.internal");
  });
});

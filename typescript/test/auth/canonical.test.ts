import { describe, expect, it } from "vitest";

import { canonicalHeaders, canonicalQuery, canonicalUri } from "../../src/auth/canonical.js";

describe("canonicalization", () => {
  it("never throws on escapes that are not UTF-8 or not escapes", () => {
    expect(canonicalUri("/a%ff/%zz/%4")).toBe("/a%FF/%25zz/%254");
    expect(canonicalQuery("k=%e1%bb%8d&k=%")).toBe("k=%25&k=%E1%BB%8D");
  });

  it("treats plus as a literal", () => {
    expect(canonicalQuery("q=a+b")).toBe("q=a%2Bb");
  });

  it("encodes raw non-ASCII as UTF-8", () => {
    expect(canonicalUri("/giọng")).toBe("/gi%E1%BB%8Dng");
  });

  it("joins repeated headers in order and rejects missing signed headers", () => {
    expect(canonicalHeaders([["X-Vgr-Meta", " a  b "], ["x-vgr-meta", "c"]], ["x-vgr-meta"])).toBe("x-vgr-meta:a b,c\n");
    expect(() => canonicalHeaders([], ["host"])).toThrow(/host/);
  });
});

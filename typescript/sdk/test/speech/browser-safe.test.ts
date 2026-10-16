import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { describe, expect, it } from "vitest";

/** Every module the entry loads, following static and dynamic relative imports. */
function reachable(entry: string): string[] {
  const seen = new Set<string>();
  const visit = (file: string) => {
    if (seen.has(file)) return;
    seen.add(file);
    const source = readFileSync(file, "utf8");
    for (const [, target] of source.matchAll(/(?:from\s+|import\(\s*)"(\.{1,2}\/[^"]+)\.js"/g))
      visit(resolve(dirname(file), `${target}.ts`));
  };
  visit(entry);
  return [...seen];
}

describe("the speech entry", () => {
  it("reaches no Node module", () => {
    const modules = reachable(join(import.meta.dirname, "../../src/services/speech/index.ts"));
    expect(modules.length).toBeGreaterThan(3);
    for (const file of modules) {
      const source = readFileSync(file, "utf8");
      expect(source, file).not.toMatch(/(?:from\s+|import\(\s*|require\(\s*)"node:/);
      expect(source, file).not.toMatch(/\bBuffer\b/);
    }
  });
});

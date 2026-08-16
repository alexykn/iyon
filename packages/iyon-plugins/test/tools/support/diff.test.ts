import { describe, expect, test } from "bun:test";
import { parseUnifiedDiff, unifiedDiff } from "../../../src/tools/support/diff.ts";

describe("tool diff support", () => {
  test("creates and parses a new-file diff", () => {
    const diff = unifiedDiff("new.txt", "", "hello\n");
    expect(diff).toContain("--- a/new.txt");
    expect(diff).toContain("+hello");
    expect(parseUnifiedDiff(diff)[0]?.lines.some((line) => line.kind === "addition")).toBe(true);
  });

  test("rejects malformed diffs for raw fallback rendering", () => {
    expect(() => parseUnifiedDiff("not a diff")).toThrow("no hunks");
  });
});

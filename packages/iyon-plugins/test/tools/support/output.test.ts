import { describe, expect, test } from "bun:test";
import { truncateHead, truncateLine, truncateTail } from "../../../src/tools/support/output.ts";

describe("tool output limits", () => {
  test("preserves head and tail boundary accounting", () => {
    expect(truncateHead("a\nb\nc", { maxLines: 2, maxBytes: 100 })).toMatchObject({ text: "a\nb", report: { truncated: true, truncatedBy: "lines", outputLines: 2 } });
    expect(truncateTail("a\nb\nc", { maxLines: 2, maxBytes: 100 })).toMatchObject({ text: "b\nc", report: { truncated: true, truncatedBy: "lines", outputLines: 2 } });
  });

  test("does not silently drop a line that exceeds the byte limit", () => {
    const result = truncateHead("abcdef\nsecond", { maxLines: 10, maxBytes: 3 });
    expect(result.text).toBe("");
    expect(result.report.firstLineExceedsLimit).toBe(true);
    expect(result.report.truncatedBy).toBe("bytes");
  });

  test("truncates grep lines by characters", () => {
    expect(truncateLine("abcdef", 3)).toEqual({ text: "abc... [truncated]", truncated: true });
  });
});

import { describe, expect, test } from "bun:test";
import { findProgram, runCapture } from "../../../src/tools/support/process.ts";

describe("tool process support", () => {
  test("captures stdout, stderr, and non-zero exit codes", async () => {
    const result = await runCapture({ program: "/bin/sh", args: ["-c", "printf out; printf err >&2; exit 1"] });
    expect(new TextDecoder().decode(result.stdout)).toBe("out");
    expect(new TextDecoder().decode(result.stderr)).toBe("err");
    expect(result.exitCode).toBe(1);
  });

  test("finds executable fallbacks and cancels during capture", async () => {
    expect(findProgram("sh")).toBeDefined();
    const controller = new AbortController();
    const pending = runCapture({ program: "/bin/sh", args: ["-c", "sleep 5"] }, controller.signal);
    controller.abort();
    await expect(pending).rejects.toThrow("cancelled");
  });
});

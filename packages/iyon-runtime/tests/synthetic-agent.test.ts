import { describe, expect, test } from "bun:test";

import { installIyonVirtualModules } from "../src/virtual-modules.ts";

installIyonVirtualModules();
const { runSyntheticAgent } = await import("./fixtures/synthetic-agent.ts");

describe("T4 synthetic agent", () => {
  test("uses only the public api/core path", async () => {
    const result: Awaited<ReturnType<typeof runSyntheticAgent>> = await runSyntheticAgent();
    expect(result.snapshot.entries.map((entry: { role?: string }) => entry.role)).toEqual([
      "user",
      "assistant",
      "toolResult",
    ]);
    expect(result.events.some((event) => event.type === "messageDelta")).toBe(true);
    expect(result.events.some((event) => event.type === "toolResultFinished")).toBe(true);
  });
});

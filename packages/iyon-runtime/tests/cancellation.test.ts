import { describe, expect, test } from "bun:test";

import { installIyonVirtualModules } from "../src/virtual-modules.ts";

installIyonVirtualModules();
const core = await import("iyon:core");

describe("T4 cancellation", () => {
  test("AbortSignal cancels a blocked native model push", async () => {
    const session = new core.KernelSession({ id: 101, eventCapacity: 1 });
    const turn = session.beginModelTurn({
      request: { messages: [], tools: [], params: {}, metadata: {} },
    });
    const controller = new AbortController();
    const pending = turn.push(
      { type: "textDelta", contentIndex: 0, delta: "blocked" },
      controller.signal,
    );
    controller.abort();
    await expect(pending).rejects.toMatchObject({ code: "ION_CANCELLED" });
    expect(session.snapshot().entries).toHaveLength(1);
    session.close();
  });
});

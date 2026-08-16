import { describe, expect, test } from "bun:test";
import { MockProvider } from "../src/provider.ts";
import type { ModelRequest } from "iyon:api";
import { collectEvents, expectEventSequence } from "../../../../packages/iyon-runtime/test/providers/fixtures.ts";

const request = (messages: ModelRequest["messages"]): ModelRequest => ({ messages, tools: [] });

describe("mock provider", () => {
  test("matches the deterministic Rust event sequence", async () => {
    const events = await collectEvents(new MockProvider({ initialDelayMs: 0, chunkDelayMs: 0 }).stream(request([
      { role: "user", content: [{ type: "text", text: "first" }] },
      { role: "user", content: [{ type: "text", text: "last" }] },
    ])));
    expectEventSequence(events, [
      { type: "started" },
      { type: "textStart", contentIndex: 0 },
      { type: "textDelta", contentIndex: 0, delta: "Mock " },
      { type: "textDelta", contentIndex: 0, delta: "response " },
      { type: "textDelta", contentIndex: 0, delta: "to: " },
      { type: "textDelta", contentIndex: 0, delta: "last" },
      { type: "textEnd", contentIndex: 0, text: "Mock response to: last" },
      { type: "done", stopReason: "stop" },
    ]);
  });

  test("uses the fallback text for empty input", async () => {
    const events = await collectEvents(new MockProvider({ initialDelayMs: 0, chunkDelayMs: 0 }).stream(request([])));
    expect(events.at(-2)).toEqual({ type: "textEnd", contentIndex: 0, text: "Mock response to: there" });
  });

  test("honors cancellation", async () => {
    const controller = new AbortController();
    controller.abort();
    const events = await collectEvents(new MockProvider({ initialDelayMs: 10 }).stream(request([]), { signal: controller.signal }));
    expect(events).toEqual([{ type: "done", stopReason: "aborted" }]);
  });
});

import { describe, expect, test } from "bun:test";
import { CoreEventMapper, coalesceFrontendEvents } from "../src/backend.ts";
import type { CoreEvent } from "@iyon/sdk";

const event = (value: unknown): CoreEvent => value as CoreEvent;

describe("core event mapper", () => {
  test("maps only role-aware message deltas and clears roles", () => {
    const mapper = new CoreEventMapper();
    expect(mapper.map(event({ type: "messageDelta", turnId: 1, messageId: 9, delta: { type: "text", text: "ignored" } }))).toBeUndefined();
    mapper.map(event({ type: "messageStarted", turnId: 1, messageId: 9, role: "assistant" }));
    expect(mapper.map(event({ type: "messageDelta", turnId: 1, messageId: 9, delta: { type: "text", text: "hello" } }))).toEqual({ type: "assistantDelta", text: "hello" });
    mapper.map(event({ type: "messageFinished", turnId: 1, messageId: 9 }));
    expect(mapper.map(event({ type: "messageDelta", turnId: 1, messageId: 9, delta: { type: "text", text: "ignored" } }))).toBeUndefined();
  });
  test("coalesces only adjacent matching deltas", () => {
    expect(coalesceFrontendEvents([
      { type: "assistantDelta", text: "a" }, { type: "assistantDelta", text: "b" },
      { type: "thinkingDelta", text: "c" }, { type: "assistantDelta", text: "d" },
    ])).toEqual([
      { type: "assistantDelta", text: "ab" }, { type: "thinkingDelta", text: "c" }, { type: "assistantDelta", text: "d" },
    ]);
  });
});

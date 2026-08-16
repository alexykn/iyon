import { describe, expect, test } from "bun:test";
import { createInitialState, reduceIyonState } from "../src/state.ts";

describe("default app integration contract", () => {
  test("consumes generic tool lifecycle without selecting a tool implementation", () => {
    let state = createInitialState({ provider: "mock", modelId: "mock" });
    state = reduceIyonState(state, { type: "backend", event: { type: "toolCallPreparing", key: { messageId: 1, contentIndex: 0 }, toolName: "custom-tool" } });
    state = reduceIyonState(state, { type: "backend", event: { type: "toolCallPrepared", key: { messageId: 1, contentIndex: 0 }, toolCallId: "custom-id", toolName: "custom-tool", arguments: {} } });
    expect([...state.liveTools.values()][0]?.toolName).toBe("custom-tool"); expect([...state.liveTools.values()][0]?.status).toBe("prepared");
  });
});

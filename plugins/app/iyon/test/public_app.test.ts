import { describe, expect, test } from "bun:test";
import { createInitialState, reduceIyonState } from "../src/state.ts";
import { cancellationTranscript, streamedToolTranscript } from "./public_app_fixtures.ts";

function replay(events: readonly import("../src/contracts.ts").FrontendEvent[]) {
  return events.reduce((state, event) => reduceIyonState(state, { type: "backend", event }), createInitialState({ provider: "mock", modelId: "mock" }));
}

describe("TS public app overlap fixtures", () => {
  test("streamed tool draft remains one generic card through approval and result", () => {
    const state = replay(streamedToolTranscript);
    expect(state.liveTools.size).toBe(1);
    expect([...state.liveTools.values()][0]).toMatchObject({ toolCallId: "call-1", status: "finished", frozen: true, isError: false });
    expect(state.pendingApproval).toBeUndefined();
    expect(state.activeTurn).toBe(false);
  });
  test("cancellation preserves buffered assistant text and clears working state", () => {
    const state = replay(cancellationTranscript);
    expect(state.assistantText).toBe("buffered"); expect(state.activeTurn).toBe(false); expect(state.working).toBe(false);
  });
});

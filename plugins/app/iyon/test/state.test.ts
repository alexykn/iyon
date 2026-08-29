import { describe, expect, test } from "bun:test";
import { createInitialState, reduceIyonState } from "../src/state.ts";

const model = { provider: "mock", modelId: "mock" } as const;

describe("iyon state", () => {
  test("tracks user batches and turn boundaries", () => {
    let state = createInitialState(model);
    state = reduceIyonState(state, { type: "submit", text: "hello" });
    expect(state.userBatches).toEqual(["hello"]); expect(state.activeTurn).toBe(true);
    state = reduceIyonState(state, { type: "backend", event: { type: "turnFinished" } });
    expect(state.activeTurn).toBe(false); expect(state.working).toBe(false);
  });
  test("rejects whitespace-only prompts and queue entries", () => {
    let state = createInitialState(model);
    state = reduceIyonState(state, { type: "submit", text: "\n  \t" });
    expect(state.userBatches).toEqual([]);
    state = reduceIyonState(state, { type: "submit", text: "hello" });
    state = reduceIyonState(state, { type: "backend", event: { type: "steerQueued", text: " \n" } });
    expect(state.steering).toEqual([]);
    state = { ...state, steering: ["\n"], steeringQueueIds: ["1"], activityVisible: true };
    state = reduceIyonState(state, { type: "backend", event: { type: "turnFinished" } });
    expect(state.steering).toEqual([]);
    expect(state.activityVisible).toBe(false);
  });

  test("uses composer, active-turn, then goodbye Ctrl+C precedence", () => {
    let state = createInitialState(model); state = reduceIyonState(state, { type: "composerPaste", text: "draft" });
    state = reduceIyonState(state, { type: "ctrlC" }); expect(state.composerText).toBe("");
    state = reduceIyonState(state, { type: "submit", text: "hello" }); const active = reduceIyonState(state, { type: "ctrlC" });
    expect(active.goodbye).toBe(false); state = reduceIyonState({ ...active, activeTurn: false }, { type: "ctrlC" }); expect(state.goodbye).toBe(true);
  });
});

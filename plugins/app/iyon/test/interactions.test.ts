import { describe, expect, test } from "bun:test";
import { handleIyonAction } from "../src/actions.ts";
import { ComposerPasteStore } from "../src/composer.ts";
import { createInitialState } from "../src/state.ts";

function context(calls: string[]) {
  return { core: { cancelActiveTurn: () => { calls.push("cancel"); }, cycleReasoningEffort: () => { calls.push("reasoning"); } }, agent: { cancel: () => { calls.push("agent-cancel"); } }, pasteStore: new ComposerPasteStore(), onExit: () => { calls.push("exit"); } };
}

describe("app interactions", () => {
  test("handles Ctrl+C precedence and Escape", async () => {
    const calls: string[] = []; const ctx = context(calls); let state = createInitialState({ provider: "mock", modelId: "mock" });
    state = { ...state, composerText: "draft" }; let result = await handleIyonAction(state, { type: "ctrlC" }, ctx); expect(result.state.composerText).toBe(""); expect(calls).toEqual([]);
    state = { ...result.state, activeTurn: true }; await handleIyonAction(state, { type: "escape" }, ctx); expect(calls).toEqual(["cancel", "agent-cancel"]);
    state = { ...state, activeTurn: false }; result = await handleIyonAction(state, { type: "ctrlC" }, ctx); expect(result.exited).toBe(true); expect(calls.at(-1)).toBe("exit");
  });
  test("cycles reasoning through the typed levels", async () => {
    const calls: string[] = []; const state = createInitialState({ provider: "mock", modelId: "mock" }); const result = await handleIyonAction(state, { type: "cycleReasoningEffort" }, context(calls));
    expect(result.state.info.reasoningEffort).toBe("high"); expect(calls).toContain("reasoning");
  });

  test("queues active-turn submits without starting another agent run", async () => {
    const calls: string[] = [];
    const contextWithRun = {
      ...context(calls),
      core: { submitTurn: (text: string) => { calls.push(`submit:${text}`); } },
      runAgent: () => { calls.push("run"); },
      clearComposer: () => { calls.push("clear"); },
    };
    const state = { ...createInitialState({ provider: "mock", modelId: "mock" }), activeTurn: true };
    const result = await handleIyonAction(state, { type: "submit", text: "steer" }, contextWithRun);
    expect(result.state.userBatches).toEqual([]);
    expect(result.state.steering).toEqual(["steer"]);
    expect(calls).toEqual(["clear", "submit:steer"]);
  });
});

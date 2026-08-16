import type { ReasoningLevel } from "@iyon/sdk";
import type { IyonAction, IyonAgent, IyonCoreCommands, IyonState } from "./contracts.ts";
import { ComposerPasteStore } from "./composer.ts";
import { reduceIyonState } from "./state.ts";

export interface ActionContext {
  readonly core: IyonCoreCommands;
  readonly agent: IyonAgent;
  readonly pasteStore: ComposerPasteStore;
  readonly clearComposer?: () => Promise<void> | void;
  readonly onExit?: () => Promise<void> | void;
}

export interface ActionResult { readonly state: IyonState; readonly exited: boolean; }

const REASONING_LEVELS: readonly ReasoningLevel[] = ["none", "minimal", "low", "medium", "high", "xhigh", "max"];

export async function handleIyonAction(state: IyonState, action: IyonAction, context: ActionContext): Promise<ActionResult> {
  if (action.type === "submit") {
    const text = context.pasteStore.expand(action.text);
    if (text.length === 0) return { state, exited: false };
    await context.core.submitTurn?.(text);
    return { state: reduceIyonState(state, { type: "submit", text }), exited: false };
  }
  if (action.type === "ctrlC") {
    if (state.composerText.length > 0) {
      context.pasteStore.clear(); await context.clearComposer?.();
      return { state: reduceIyonState(state, action), exited: false };
    }
    if (state.activeTurn) {
      await context.core.cancelActiveTurn?.(); await context.agent.cancel?.();
      return { state, exited: false };
    }
    await context.onExit?.();
    return { state: reduceIyonState(state, { type: "requestExit" }), exited: true };
  }
  if (action.type === "escape") {
    if (state.activeTurn) { await context.core.cancelActiveTurn?.(); await context.agent.cancel?.(); }
    return { state, exited: false };
  }
  if (action.type === "cycleReasoningEffort") {
    const index = REASONING_LEVELS.indexOf(state.info.reasoningEffort);
    const next = REASONING_LEVELS[(index + 1) % REASONING_LEVELS.length];
    await context.core.cycleReasoningEffort?.();
    return { state: reduceIyonState(state, { type: "backend", event: { type: "configChanged", provider: state.info.provider, modelId: state.info.modelId, reasoningEffort: next } }), exited: false };
  }
  if (action.type === "approve") { await context.core.approve?.(action.approvalId); return { state, exited: false }; }
  if (action.type === "reject") { await context.core.reject?.(action.approvalId, action.reason); return { state, exited: false }; }
  return { state: reduceIyonState(state, action), exited: false };
}


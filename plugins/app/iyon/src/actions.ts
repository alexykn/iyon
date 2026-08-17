import type { ReasoningLevel } from "@iyon/sdk";
import type { IyonAction, IyonAgent, IyonCoreCommands, IyonState } from "./contracts.ts";
import { ComposerPasteStore } from "./composer.ts";
import { hasActiveWork, reduceIyonState } from "./state.ts";

export interface ActionContext {
  readonly core: IyonCoreCommands;
  readonly agent: IyonAgent;
  readonly pasteStore: ComposerPasteStore;
  readonly clearComposer?: () => Promise<void> | void;
  readonly composerText?: () => Promise<string> | string;
  readonly forwardPaste?: (text: string) => Promise<void> | void;
  readonly runAgent?: () => Promise<unknown> | void;
  readonly onExit?: () => Promise<void> | void;
  readonly isAgentRunning?: () => boolean;
}

export interface ActionResult { readonly state: IyonState; readonly exited: boolean; readonly queueId?: number; readonly cancelledWork?: boolean; }

const REASONING_LEVELS: readonly ReasoningLevel[] = ["none", "minimal", "low", "medium", "high", "xhigh", "max"];

export async function handleIyonAction(state: IyonState, action: IyonAction, context: ActionContext): Promise<ActionResult> {
  if (action.type === "submit") {
    const text = context.pasteStore.expand(action.text);
    if (text.length === 0) return { state, exited: false };
    const steering = hasActiveWork(state);
    await context.clearComposer?.();
    const queueId = steering ? await submitSteering(context.core, text) : await submitPrompt(context.core, text);
    if (!steering) await context.runAgent?.();
    return {
      state: steering
        ? reduceIyonState(state, { type: "backend", event: { type: "steerQueued", text, queueId } })
        : reduceIyonState(state, { type: "submit", text, queueId }),
      exited: false,
      queueId,
    };
  }
  if (action.type === "ctrlC") {
    if (hasActiveWork(state) || context.isAgentRunning?.()) return await cancelActiveWork(state, context);
    const composerText = await context.composerText?.() ?? state.composerText;
    if (composerText.length > 0) {
      context.pasteStore.clear(); await context.clearComposer?.();
      return { state: reduceIyonState(state, action), exited: false };
    }
    await context.onExit?.();
    return { state: reduceIyonState(state, { type: "requestExit" }), exited: true };
  }
  if (action.type === "escape") {
    if (hasActiveWork(state) || context.isAgentRunning?.()) return await cancelActiveWork(state, context);
    return { state, exited: false };
  }
  if (action.type === "cycleReasoningEffort") {
    const index = REASONING_LEVELS.indexOf(state.info.reasoningEffort);
    const next = REASONING_LEVELS[(index + 1) % REASONING_LEVELS.length];
    if (context.core.setReasoningEffort !== undefined) await context.core.setReasoningEffort(next);
    else await context.core.cycleReasoningEffort?.();
    return { state: reduceIyonState(state, { type: "backend", event: { type: "configChanged", provider: state.info.provider, modelId: state.info.modelId, reasoningEffort: next } }), exited: false };
  }
  if (action.type === "composerPaste") {
    const current = await context.composerText?.() ?? state.composerText;
    const display = context.pasteStore.displayText(current, action.text);
    await context.forwardPaste?.(display);
    return { state: reduceIyonState(state, { type: "composerPaste", text: display }), exited: false };
  }
  if (action.type === "approve") { await context.core.approve?.(action.approvalId); return { state, exited: false }; }
  if (action.type === "reject") { await context.core.reject?.(action.approvalId, action.reason); return { state, exited: false }; }
  return { state: reduceIyonState(state, action), exited: false };
}

async function cancelActiveWork(state: IyonState, context: ActionContext): Promise<ActionResult> {
  await context.core.cancelActiveTurn?.();
  await context.agent.cancel?.();
  return { state: reduceIyonState(state, { type: "backend", event: { type: "turnCancelled" } }), exited: false, cancelledWork: true };
}

async function submitPrompt(core: IyonCoreCommands, text: string): Promise<number | undefined> {
  if (core.submitPrompt !== undefined) return await core.submitPrompt(text);
  await core.submitTurn?.(text);
  return undefined;
}

async function submitSteering(core: IyonCoreCommands, text: string): Promise<number | undefined> {
  if (core.steer !== undefined) return await core.steer(text);
  if (core.followUp !== undefined) return await core.followUp(text);
  await core.submitTurn?.(text);
  return undefined;
}

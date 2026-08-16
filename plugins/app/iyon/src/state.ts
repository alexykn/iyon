import type { ReasoningLevel } from "@iyon/sdk";
import type { FrontendEvent, InfoState, IyonAction, IyonModelMetadata, IyonState, LiveTool, ToolUpdatePresentation } from "./contracts.ts";

export function createInitialState(model: IyonModelMetadata): IyonState {
  return {
    info: { status: "", provider: model.provider, modelId: model.modelId, reasoningEffort: model.reasoningEffort ?? "none" },
    composerText: "", userBatches: [], working: false, steering: [], assistantText: "", thinkingText: "",
    liveTools: new Map(), draftTools: new Map(), activeTurn: false, goodbye: false,
  };
}

export function reduceIyonState(state: IyonState, action: IyonAction): IyonState {
  if (action.type === "submit") return action.text.length === 0 ? state : { ...state, composerText: "", userBatches: [...state.userBatches, action.text], activeTurn: true, working: true };
  if (action.type === "composerPaste") return { ...state, composerText: action.text };
  if (action.type === "requestExit") return { ...state, goodbye: true };
  if (action.type === "ctrlC") {
    if (state.composerText.length > 0) return { ...state, composerText: "" };
    if (state.activeTurn) return state;
    return { ...state, goodbye: true };
  }
  if (action.type === "backend") return reduceFrontendEvent(state, action.event);
  return state;
}

export function updateInfo(state: IyonState, info: Partial<InfoState>): IyonState { return { ...state, info: { ...state.info, ...info } }; }
export function cycleReasoningEffort(state: IyonState, next: ReasoningLevel): IyonState { return updateInfo(state, { reasoningEffort: next }); }

function reduceFrontendEvent(state: IyonState, event: FrontendEvent): IyonState {
  switch (event.type) {
    case "turnStarted": return { ...state, activeTurn: true, working: true };
    case "userMessage": return { ...state, userBatches: [...state.userBatches, event.text] };
    case "assistantDelta": return { ...state, assistantText: state.assistantText + event.text };
    case "thinkingDelta": return { ...state, thinkingText: state.thinkingText + event.text };
    case "steerQueued": return { ...state, steering: [...state.steering, event.text] };
    case "configChanged": return updateInfo(state, { provider: event.provider, modelId: event.modelId, reasoningEffort: event.reasoningEffort });
    case "toolCallPreparing": return withDraft(state, event.key, { draftKey: event.key, toolCallId: event.toolCallId, toolName: event.toolName, status: "preparing", text: "", isError: false, frozen: false });
    case "toolCallArguments": return updateDraft(state, event.key, (tool) => ({ ...tool, text: tool.text + event.delta, toolCallId: event.toolCallId ?? tool.toolCallId, toolName: event.toolName ?? tool.toolName }));
    case "toolCallPrepared": return updateDraft(state, event.key, (tool) => ({ ...tool, toolCallId: event.toolCallId, toolName: event.toolName, arguments: event.arguments, status: "prepared" }));
    case "toolCallStarted": return updateTool(state, event.toolCallId, (tool) => ({ ...tool, toolCallId: event.toolCallId, toolName: event.toolName, arguments: event.arguments, status: "running" }));
    case "toolCallUpdated": return updateTool(state, event.toolCallId, (tool) => applyToolUpdate(tool, event.update));
    case "toolApprovalRequested": return updateTool(state, event.toolCallId, (tool) => ({ ...tool, status: "pendingApproval" }));
    case "toolApprovalResolved": return updateTool(state, event.toolCallId, (tool) => ({ ...tool, status: event.approved ? "running" : "cancelled", frozen: !event.approved }));
    case "toolResult": return updateTool(state, event.toolCallId, (tool) => ({ ...tool, toolName: event.toolName, text: event.text, details: event.details, status: event.isError ? "failed" : "finished", isError: event.isError, frozen: true }));
    case "toolCallFinished": return updateTool(state, event.toolCallId, (tool) => ({ ...tool, status: event.isError ? "failed" : "finished", isError: event.isError, frozen: true }));
    case "turnFinished": return { ...state, activeTurn: false, working: false, steering: [] };
    case "turnFailed": return { ...state, activeTurn: false, working: false, info: { ...state.info, status: event.message } };
    case "turnCancelled": return { ...state, activeTurn: false, working: false };
  }
}

function withDraft(state: IyonState, key: { readonly messageId: number; readonly contentIndex: number }, tool: LiveTool): IyonState {
  const id = draftIdFor(key);
  return { ...state, liveTools: new Map(state.liveTools).set(id, tool), draftTools: new Map(state.draftTools).set(id, id) };
}
function updateDraft(state: IyonState, key: { readonly messageId: number; readonly contentIndex: number }, update: (tool: LiveTool) => LiveTool): IyonState {
  const id = draftIdFor(key); const tool = state.liveTools.get(id);
  return tool === undefined ? state : { ...state, liveTools: new Map(state.liveTools).set(id, update(tool)) };
}
function updateTool(state: IyonState, toolCallId: string, update: (tool: LiveTool) => LiveTool): IyonState {
  const match = [...state.liveTools.entries()].find(([, tool]) => tool.toolCallId === toolCallId);
  return match === undefined ? state : { ...state, liveTools: new Map(state.liveTools).set(match[0], update(match[1])) };
}
function applyToolUpdate(tool: LiveTool, update: ToolUpdatePresentation): LiveTool {
  if (update.type === "text") return { ...tool, text: tool.text + update.text };
  if (update.type === "progress") return { ...tool, progress: update };
  return { ...tool, details: update.details };
}
export function draftIdFor(key: { readonly messageId: number; readonly contentIndex: number }): string { return `${key.messageId}:${key.contentIndex}`; }

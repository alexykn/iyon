import type { ReasoningLevel } from "@iyon/sdk";
import type { FrontendEvent, InfoState, IyonAction, IyonModelMetadata, IyonState, LiveTool, ToolUpdatePresentation } from "./contracts.ts";

export function createInitialState(model: IyonModelMetadata): IyonState {
  return {
    info: { status: "", provider: model.provider, modelId: model.modelId, reasoningEffort: model.reasoningEffort ?? "medium" },
    composerText: "", userBatches: [], working: false, steering: [], assistantText: "", thinkingText: "", assistantOpen: false,
    liveTools: new Map(), draftTools: new Map(), activeTurn: false, goodbye: false,
  };
}

export function hasActiveWork(state: IyonState): boolean {
  return state.activeTurn
    || state.assistantOpen
    || [...state.liveTools.values()].some((tool) => !tool.frozen)
    || state.pendingApproval !== undefined
    || state.working;
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
    case "assistantDelta": return { ...state, assistantText: state.assistantText + event.text, assistantOpen: true };
    case "thinkingDelta": return { ...state, thinkingText: state.thinkingText + event.text, assistantOpen: true };
    case "steerQueued": return { ...state, steering: [...state.steering, event.text] };
    case "configChanged": return updateInfo(state, { provider: event.provider, modelId: event.modelId, reasoningEffort: event.reasoningEffort });
    case "toolCallPreparing": return withDraft(state, event.key, { draftKey: event.key, toolCallId: event.toolCallId, toolName: event.toolName, status: "preparing", text: "", isError: false, frozen: false });
    case "toolCallArguments": return updateDraft(state, event.key, (tool) => ({ ...tool, text: tool.text + event.delta, toolCallId: event.toolCallId ?? tool.toolCallId, toolName: event.toolName ?? tool.toolName }));
    case "toolCallPrepared": return updateDraft(state, event.key, (tool) => ({ ...tool, toolCallId: event.toolCallId, toolName: event.toolName, arguments: event.arguments, status: "prepared" }));
    case "toolCallStarted": return startTool(state, event.toolCallId, event.toolName, event.arguments);
    case "toolCallUpdated": return updateTool(state, event.toolCallId, (tool) => applyToolUpdate(tool, event.update));
    case "toolApprovalRequested": return { ...updateTool(state, event.toolCallId, (tool) => ({ ...tool, status: "pendingApproval" })), pendingApproval: { approvalId: event.approvalId, toolCallId: event.toolCallId, toolName: event.toolName, arguments: event.arguments } };
    case "toolApprovalResolved": return { ...updateTool(state, event.toolCallId, (tool) => ({ ...tool, status: event.approved ? "running" : "cancelled", frozen: !event.approved })), pendingApproval: undefined };
    case "toolResult": return updateTool(state, event.toolCallId, (tool) => ({ ...tool, toolName: event.toolName, text: event.text, details: event.details, status: event.isError ? "failed" : "finished", isError: event.isError, frozen: true }));
    case "toolCallFinished": return updateTool(state, event.toolCallId, (tool) => ({ ...tool, status: event.isError ? "failed" : "finished", isError: event.isError, frozen: true }));
    case "turnFinished": return { ...state, activeTurn: false, assistantOpen: false, working: false, steering: [], liveTools: finalizeLiveTools(state.liveTools) };
    case "turnFailed": return { ...state, activeTurn: false, assistantOpen: false, working: false, liveTools: finalizeLiveTools(state.liveTools), info: { ...state.info, status: event.message } };
    case "turnCancelled": return { ...state, activeTurn: false, assistantOpen: false, working: false, liveTools: cancelLiveTools(state.liveTools) };
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

function startTool(state: IyonState, toolCallId: string, toolName: string, argumentsValue: import("@iyon/sdk").JsonValue): IyonState {
  const match = [...state.liveTools.entries()].find(([, tool]) => tool.toolCallId === toolCallId);
  if (match !== undefined) return { ...state, liveTools: new Map(state.liveTools).set(match[0], { ...match[1], toolCallId, toolName, arguments: argumentsValue, status: "running" }) };
  return { ...state, liveTools: new Map(state.liveTools).set(toolCallId, { toolCallId, toolName, arguments: argumentsValue, status: "running", text: "", isError: false, frozen: false }) };
}
function applyToolUpdate(tool: LiveTool, update: ToolUpdatePresentation): LiveTool {
  if (update.type === "text") return { ...tool, text: tool.text + update.text };
  if (update.type === "progress") return { ...tool, progress: update };
  return { ...tool, details: update.details };
}
export function draftIdFor(key: { readonly messageId: number; readonly contentIndex: number }): string { return `${key.messageId}:${key.contentIndex}`; }

function cancelLiveTools(tools: ReadonlyMap<string, LiveTool>): ReadonlyMap<string, LiveTool> {
  return new Map([...tools].map(([key, tool]) => [key, tool.frozen ? tool : { ...tool, status: "cancelled" as const, frozen: true, isError: true }]));
}

function finalizeLiveTools(tools: ReadonlyMap<string, LiveTool>): ReadonlyMap<string, LiveTool> {
  return new Map([...tools].map(([key, tool]) => [key, tool.frozen ? tool : { ...tool, status: "failed" as const, frozen: true, isError: true }]));
}

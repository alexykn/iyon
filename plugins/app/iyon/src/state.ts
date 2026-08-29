import type { ReasoningLevel } from "@iyon/sdk";
import type { FrontendEvent, InfoState, IyonAction, IyonModelMetadata, IyonState, LiveTool, ToolUpdatePresentation } from "./contracts.ts";

export function createInitialState(model: IyonModelMetadata): IyonState {
  return {
    info: { status: "", provider: model.provider, modelId: model.modelId, reasoningEffort: model.reasoningEffort ?? "medium" },
    composerText: "", userBatches: [], working: false, activityVisible: false, steering: [], steeringQueueIds: [], assistantText: "", thinkingText: "", assistantOpen: false,
    liveTools: new Map(), draftTools: new Map(), activeTurn: false, goodbye: false,
  };
}

export function isBlankText(text: string): boolean {
  return text.trim().length === 0;
}

export function hasActiveWork(state: IyonState): boolean {
  return state.activeTurn
    || state.assistantOpen
    || [...state.liveTools.values()].some((tool) => !tool.frozen)
    || state.pendingApproval !== undefined
    || state.working;
}

export function reduceIyonState(state: IyonState, action: IyonAction): IyonState {
  if (action.type === "submit") {
    if (isBlankText(action.text)) return removeBlankSteering(state);
    return { ...state, composerText: "", userBatches: [...state.userBatches, action.text], activeTurn: true, working: true, activityVisible: true };
  }
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
  state = removeBlankSteering(state);
  switch (event.type) {
    case "turnStarted": {
      const activityVisible = state.activeTurn || state.activityVisible || state.steering.length > 0;
      return { ...state, activeTurn: true, working: state.working || activityVisible, activityVisible };
    }
    case "userMessage": {
      if (isBlankText(event.text)) return state;
      const delivered = deliverQueuedSteer(state, event.text, event.queueId);
      return { ...state, userBatches: [...state.userBatches, event.text], activityVisible: state.activeTurn, ...delivered };
    }
    case "assistantDelta": return { ...state, assistantText: state.assistantText + event.text, assistantOpen: true, activityVisible: state.activeTurn && state.steering.length > 0 };
    case "thinkingDelta": return { ...state, thinkingText: state.thinkingText + event.text, assistantOpen: true, activityVisible: state.activeTurn && state.steering.length > 0 };
    case "steerQueued": return enqueueSteer(state, event.text, event.queueId);
    case "configChanged": return updateInfo(state, { provider: event.provider, modelId: event.modelId, reasoningEffort: event.reasoningEffort });
    case "toolCallPreparing": return withDraft(showActivity(state), event.key, { draftKey: event.key, toolCallId: normalizeToolCallId(event.toolCallId), toolName: event.toolName, argumentPreview: "", status: "preparing", isError: false, frozen: false });
    case "toolCallArguments": return updateDraft(showActivity(state), event.key, (tool) => ({ ...tool, argumentPreview: tool.argumentPreview + event.delta, toolCallId: event.toolCallId === undefined ? tool.toolCallId : normalizeToolCallId(event.toolCallId), toolName: event.toolName ?? tool.toolName }));
    case "toolCallPrepared": return updateDraft(showActivity(state), event.key, (tool) => ({ ...tool, toolCallId: normalizeToolCallId(event.toolCallId)!, toolName: event.toolName, arguments: event.arguments, status: "prepared" }));
    case "toolCallStarted": return startTool(showActivity(state), normalizeToolCallId(event.toolCallId)!, event.toolName, event.arguments);
    case "toolCallUpdated": return updateTool(showActivity(state), normalizeToolCallId(event.toolCallId)!, (tool) => applyToolUpdate(tool, event.update));
    case "toolApprovalRequested": return { ...updateTool(showActivity(state), normalizeToolCallId(event.toolCallId)!, (tool) => ({ ...tool, status: "pendingApproval" })), pendingApproval: { approvalId: event.approvalId, toolCallId: normalizeToolCallId(event.toolCallId)!, toolName: event.toolName, arguments: event.arguments } };
    case "toolApprovalResolved": return { ...updateTool(showActivity(state), normalizeToolCallId(event.toolCallId)!, (tool) => ({ ...tool, status: event.approved ? "running" : "cancelled", frozen: !event.approved })), pendingApproval: undefined };
    case "toolResult": return updateTool(showActivity(state), normalizeToolCallId(event.toolCallId)!, (tool) => ({ ...tool, toolName: event.toolName, result: { content: [{ type: "text", text: event.text }], details: event.details, isError: event.isError, toolCallId: normalizeToolCallId(event.toolCallId) as never, toolName: event.toolName, text: event.text }, status: event.isError ? "failed" : "finished", isError: event.isError, frozen: true }));
    case "toolCallFinished": return state;
    case "turnFinished": {
      // A steer can be accepted at the same boundary where the current turn
      // emits TurnFinished. Keep its preview until the matching userMessage
      // arrives; otherwise the waiting row flickers and disappears.
      if (state.steering.length > 0 || hasPreparedTool(state)) {
        return { ...state, activeTurn: true, assistantOpen: false, working: true, activityVisible: true };
      }
      return { ...state, activeTurn: false, assistantOpen: false, working: false, activityVisible: false, steering: [], steeringQueueIds: [], liveTools: finalizeLiveTools(state.liveTools) };
    }
    case "turnFailed": return { ...state, activeTurn: false, assistantOpen: false, working: false, activityVisible: false, steering: [], steeringQueueIds: [], liveTools: finalizeLiveTools(state.liveTools), info: { ...state.info, status: event.message } };
    case "turnCancelled": return { ...state, activeTurn: false, assistantOpen: false, working: false, activityVisible: false, steering: [], steeringQueueIds: [], liveTools: cancelLiveTools(state.liveTools) };
  }
}

function showActivity(state: IyonState): IyonState {
  return { ...state, activeTurn: true, working: true, activityVisible: true };
}

function removeBlankSteering(state: IyonState): IyonState {
  if (!state.steering.some(isBlankText)) return state;
  const entries = state.steering
    .map((text, index) => ({ text, queueId: state.steeringQueueIds[index] }))
    .filter(({ text }) => !isBlankText(text));
  return {
    ...state,
    steering: entries.map(({ text }) => text),
    steeringQueueIds: entries.map(({ queueId }) => queueId),
  };
}

/**
 * The submit action adds a local queue preview immediately, while core also
 * emits SteerQueued as the authoritative acknowledgement. Reconcile those
 * two paths instead of appending the same message twice.
 */
function enqueueSteer(state: IyonState, text: string, rawQueueId?: string | number): IyonState {
  if (isBlankText(text)) return state;
  const queueId = rawQueueId === undefined ? undefined : String(rawQueueId);
  if (queueId !== undefined && state.steeringQueueIds.includes(queueId)) return state;
  if (queueId === undefined && state.steering.some((item, index) => item === text && state.steeringQueueIds[index] === undefined)) return state;

  // A fallback core may not return an ID locally, while its event still does.
  // Upgrade the matching local preview in place so later delivery can remove
  // the exact queue item without collapsing identical messages.
  if (queueId !== undefined) {
    const localIndex = state.steering.findIndex((item, index) => item === text && state.steeringQueueIds[index] === undefined);
    if (localIndex >= 0) {
      const steeringQueueIds = [...state.steeringQueueIds];
      steeringQueueIds[localIndex] = queueId;
      return { ...state, steeringQueueIds };
    }
  }
  return { ...state, steering: [...state.steering, text], steeringQueueIds: [...state.steeringQueueIds, queueId], activityVisible: state.activeTurn };
}

function hasPreparedTool(state: IyonState): boolean {
  return [...state.liveTools.values()].some((tool) => !tool.frozen && (tool.status === "preparing" || tool.status === "prepared"));
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
  const match = [...state.liveTools.entries()].find(([, tool]) => tool.toolCallId !== undefined && normalizeToolCallId(tool.toolCallId) === toolCallId);
  return match === undefined ? state : { ...state, liveTools: new Map(state.liveTools).set(match[0], update(match[1])) };
}

function startTool(state: IyonState, toolCallId: string, toolName: string, argumentsValue: import("@iyon/sdk").JsonValue): IyonState {
  const match = [...state.liveTools.entries()].find(([, tool]) => tool.toolCallId === toolCallId);
  if (match !== undefined) return { ...state, liveTools: new Map(state.liveTools).set(match[0], { ...match[1], toolCallId, toolName, arguments: argumentsValue, status: "running" }) };
  return { ...state, liveTools: new Map(state.liveTools).set(toolCallId, { toolCallId, toolName, arguments: argumentsValue, argumentPreview: "", status: "running", isError: false, frozen: false }) };
}
function normalizeToolCallId(toolCallId: string | number | undefined): string | undefined {
  return toolCallId === undefined ? undefined : String(toolCallId);
}
function applyToolUpdate(tool: LiveTool, update: ToolUpdatePresentation): LiveTool {
  if (update.type === "text") return { ...tool, update };
  if (update.type === "progress") return { ...tool, progress: update };
  return { ...tool, update };
}
export function draftIdFor(key: { readonly messageId: number; readonly contentIndex: number }): string { return `${key.messageId}:${key.contentIndex}`; }

function deliverQueuedSteer(state: IyonState, text: string, queueId?: string | number): Pick<IyonState, "steering" | "steeringQueueIds"> {
  const id = queueId === undefined ? undefined : String(queueId);
  let index = id === undefined ? -1 : state.steeringQueueIds.indexOf(id);
  if (index < 0) index = state.steering.indexOf(text);
  if (index < 0) return { steering: state.steering, steeringQueueIds: state.steeringQueueIds };
  return {
    steering: state.steering.filter((_, itemIndex) => itemIndex !== index),
    steeringQueueIds: state.steeringQueueIds.filter((_, itemIndex) => itemIndex !== index),
  };
}

function cancelLiveTools(tools: ReadonlyMap<string, LiveTool>): ReadonlyMap<string, LiveTool> {
  return new Map([...tools].map(([key, tool]) => [key, tool.frozen ? tool : { ...tool, status: "cancelled" as const, frozen: true, isError: true }]));
}

function finalizeLiveTools(tools: ReadonlyMap<string, LiveTool>): ReadonlyMap<string, LiveTool> {
  return new Map([...tools].map(([key, tool]) => {
    if (tool.frozen || tool.status === "preparing" || tool.status === "prepared" || tool.status === "pendingApproval") return [key, tool];
    return [key, { ...tool, status: "failed" as const, frozen: true, isError: true }];
  }));
}

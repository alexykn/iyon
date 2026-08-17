import type { CoreEvent, MessageDelta, MessageRole, ToolCallDelta, ToolUpdateEvent, ReasoningLevel } from "@iyon/sdk";
import type { FrontendEvent, ToolUpdatePresentation } from "./contracts.ts";

export interface CoreEventSource {
  nextEvent(signal?: AbortSignal): Promise<CoreEvent | null>;
  close?(): void;
}

export interface FrontendDispatcher {
  dispatch(action: { readonly type: "backend"; readonly event: FrontendEvent }): void;
}

export interface CoreEventBridge {
  readonly signal: AbortSignal;
  readonly done: Promise<void>;
  close(): void;
}

export function startCoreEventBridge(source: CoreEventSource, dispatcher: FrontendDispatcher): CoreEventBridge {
  const controller = new AbortController();
  const done = consumeCoreEvents(source, dispatcher, controller.signal);
  return {
    signal: controller.signal,
    done,
    close() { controller.abort(); source.close?.(); },
  };
}

async function consumeCoreEvents(source: CoreEventSource, dispatcher: FrontendDispatcher, signal: AbortSignal): Promise<void> {
  const mapper = new CoreEventMapper();
  while (!signal.aborted) {
    let event: CoreEvent | null;
    try {
      event = await source.nextEvent(signal);
    } catch (error) {
      if (signal.aborted || isAbortError(error)) return;
      throw error;
    }
    if (event === null) return;
    const mapped = mapper.map(event);
    if (mapped !== undefined && !signal.aborted) dispatcher.dispatch({ type: "backend", event: mapped });
  }
}

function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === "AbortError" || error instanceof Error && error.name === "AbortError";
}

export class CoreEventMapper {
  private readonly messageRoles = new Map<number, MessageRole>();

  map(event: CoreEvent): FrontendEvent | undefined {
    switch (event.type) {
      case "agentStarted":
      case "agentFinished":
      case "toolResultStarted":
        return undefined;
      case "turnStarted": return { type: "turnStarted" };
      case "steerQueued": return { type: "steerQueued", text: event.text, queueId: event.queueId };
      case "messageStarted": this.messageRoles.set(event.messageId, event.role); return undefined;
      case "messageFinished": this.messageRoles.delete(event.messageId); return undefined;
      case "messageDelta": return this.mapMessageDelta(event.messageId, event.delta);
      case "toolCallStarted": return { type: "toolCallStarted", toolCallId: event.toolCallId, toolName: event.toolName, arguments: event.arguments };
      case "toolCallUpdated": return { type: "toolCallUpdated", toolCallId: event.toolCallId, update: lowerToolUpdate(event.update) };
      case "toolCallFinished": return { type: "toolCallFinished", toolCallId: event.toolCallId, isError: event.isError };
      case "toolApprovalRequested": return { type: "toolApprovalRequested", approvalId: event.approvalId, toolCallId: event.toolCallId, toolName: event.toolName, arguments: event.arguments };
      case "toolApprovalResolved": return { type: "toolApprovalResolved", approvalId: event.approvalId, toolCallId: event.toolCallId, approved: event.approved, reason: event.reason };
      case "toolResultFinished": return { type: "toolResult", toolCallId: event.toolCallId, toolName: event.toolName, text: event.text, details: event.details, isError: event.isError };
      case "turnFinished": return { type: "turnFinished" };
      case "turnFailed": return { type: "turnFailed", message: event.message };
      case "turnCancelled": this.messageRoles.clear(); return { type: "turnCancelled" };
      case "configChanged": return { type: "configChanged", provider: event.provider, modelId: event.modelId, reasoningEffort: event.reasoningEffort as ReasoningLevel };
    }
  }

  private mapMessageDelta(messageId: number, delta: MessageDelta): FrontendEvent | undefined {
    const role = this.messageRoles.get(messageId);
    if (delta.type === "text") {
      if (role === "user") return { type: "userMessage", text: delta.text };
      if (role === "assistant") return { type: "assistantDelta", text: delta.text };
      return undefined;
    }
    if (delta.type === "thinking") return role === "assistant" ? { type: "thinkingDelta", text: delta.text } : undefined;
    if (role !== "assistant") return undefined;
    return this.mapToolDelta(messageId, delta.delta);
  }

  private mapToolDelta(messageId: number, delta: ToolCallDelta): FrontendEvent {
    const key = { messageId, contentIndex: delta.contentIndex };
    if (delta.type === "start") return { type: "toolCallPreparing", key, toolCallId: delta.toolCallId, toolName: delta.toolName };
    if (delta.type === "arguments") return { type: "toolCallArguments", key, toolCallId: delta.toolCallId, toolName: delta.toolName, delta: delta.delta };
    return { type: "toolCallPrepared", key, toolCallId: delta.toolCallId, toolName: delta.toolName, arguments: delta.arguments };
  }
}

function lowerToolUpdate(update: ToolUpdateEvent): ToolUpdatePresentation {
  if (update.type === "text") return { type: "text", text: update.text };
  if (update.type === "progress") return { type: "progress", label: update.label, current: update.current, total: update.total };
  return { type: "details", details: update.details };
}

export function coalesceFrontendEvents(events: Iterable<FrontendEvent>): FrontendEvent[] {
  const result: FrontendEvent[] = [];
  for (const event of events) {
    const previous = result.at(-1);
    if (event.type === "assistantDelta" && previous?.type === "assistantDelta") { result[result.length - 1] = { ...previous, text: previous.text + event.text }; continue; }
    if (event.type === "thinkingDelta" && previous?.type === "thinkingDelta") { result[result.length - 1] = { ...previous, text: previous.text + event.text }; continue; }
    if (event.type === "toolCallArguments" && previous?.type === "toolCallArguments" && sameDraft(previous.key, event.key)) {
      result[result.length - 1] = { ...previous, delta: previous.delta + event.delta, toolCallId: previous.toolCallId ?? event.toolCallId, toolName: previous.toolName ?? event.toolName };
      continue;
    }
    result.push(event);
  }
  return result;
}

function sameDraft(left: { readonly messageId: number; readonly contentIndex: number }, right: { readonly messageId: number; readonly contentIndex: number }): boolean {
  return left.messageId === right.messageId && left.contentIndex === right.contentIndex;
}

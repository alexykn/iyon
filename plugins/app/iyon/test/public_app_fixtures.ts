import type { FrontendEvent, ToolDraftKey } from "../src/contracts.ts";

export const draft = (messageId: number, contentIndex: number): ToolDraftKey => ({ messageId, contentIndex });

export const streamedToolTranscript: readonly FrontendEvent[] = [
  { type: "turnStarted" },
  { type: "userMessage", text: "run it" },
  { type: "assistantDelta", text: "Preparing" },
  { type: "toolCallPreparing", key: draft(1, 0), toolName: "generic" },
  { type: "toolCallArguments", key: draft(1, 0), delta: "{}" },
  { type: "toolCallPrepared", key: draft(1, 0), toolCallId: "call-1", toolName: "generic", arguments: {} },
  { type: "toolCallStarted", toolCallId: "call-1", toolName: "generic", arguments: {} },
  { type: "toolApprovalRequested", approvalId: 1, toolCallId: "call-1", toolName: "generic", arguments: {} },
  { type: "toolApprovalResolved", approvalId: 1, toolCallId: "call-1", approved: true },
  { type: "toolResult", toolCallId: "call-1", toolName: "generic", text: "done", details: {}, isError: false },
  { type: "turnFinished" },
];

export const cancellationTranscript: readonly FrontendEvent[] = [
  { type: "turnStarted" },
  { type: "assistantDelta", text: "buffered" },
  { type: "turnCancelled" },
];

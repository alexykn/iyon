import type { ContentBlock, ModelMessage, ModelRequest } from "iyon:api";
import type { SessionEntry } from "@iyon/sdk";
import {
  activeModelTools,
  contextMetadata,
  contextParams,
  contextSystemPrompt,
  selectContext,
  type AgentPolicyContext,
} from "./context.ts";

export function buildModelRequest(context: AgentPolicyContext): ModelRequest {
  const request: ModelRequest = {
    messages: selectContext(context).flatMap(lowerEntry),
    tools: activeModelTools(context),
    params: contextParams(context),
    metadata: contextMetadata(context),
  };
  const systemPrompt = contextSystemPrompt(context);
  if (systemPrompt !== undefined) request.systemPrompt = systemPrompt;
  return request;
}

function lowerEntry(entry: SessionEntry): ModelMessage[] {
  if (entry.kind !== "message") return [];
  switch (entry.role) {
    case "user":
      return [{ role: "user", content: entry.content }];
    case "assistant":
      return [{ role: "assistant", content: entry.content }];
    case "toolResult":
      return [{ role: "toolResult", toolCallId: entry.toolCallId, toolName: entry.toolName, content: entry.content, isError: entry.isError }];
    case "status":
      return [];
  }
}

export function lowerContent(content: readonly ContentBlock[]): ContentBlock[] {
  return [...content];
}

import type { ModelMetadata, ModelParams, ModelRequest, ModelToolSpec, ReasoningLevel } from "iyon:api";
import type { KernelSession, SessionSnapshot, ToolDefinition, ToolUpdateSink } from "@iyon/sdk";
import { buildSystemPrompt } from "./prompt.ts";
import { selectReasoningEffort } from "./reasoning.ts";

export interface PublicToolRegistry {
  list(): readonly { readonly value: unknown }[];
}

export interface AgentPolicyContext {
  readonly session: KernelSession;
  readonly updates?: ToolUpdateSink;
  readonly systemPrompt?: string;
  readonly metadata?: Omit<ModelMetadata, "sessionId">;
  readonly reasoningEffort?: ReasoningLevel;
  readonly tools?: PublicToolRegistry;
  readonly activeToolNames?: readonly string[];
}

export function selectCanonicalEntries(snapshot: SessionSnapshot): SessionSnapshot["entries"] {
  return snapshot.entries.filter((entry) => entry.kind === "message" && entry.role !== "status");
}

export function selectContext(context: AgentPolicyContext): SessionSnapshot["entries"] {
  return selectCanonicalEntries(context.session.snapshot());
}

export function activeModelTools(context: AgentPolicyContext): ModelToolSpec[] {
  const registered = context.tools?.list() ?? [];
  const activeNames = context.activeToolNames === undefined ? undefined : new Set(context.activeToolNames);
  return registered
    .map((entry) => entry.value)
    .filter(isToolDefinition)
    .filter((tool) => activeNames === undefined || activeNames.has(tool.name))
    .sort((left, right) => left.name.localeCompare(right.name))
    .map((tool) => tool.modelSpec ?? { name: tool.name, description: tool.description, inputSchema: tool.inputSchema });
}

export function contextParams(context: AgentPolicyContext): ModelParams {
  return selectReasoningEffort(context.reasoningEffort);
}

export function contextMetadata(context: AgentPolicyContext): ModelMetadata {
  const snapshot = context.session.snapshot();
  return {
    sessionId: String(snapshot.sessionId),
    ...context.metadata,
  };
}

export function contextSystemPrompt(context: AgentPolicyContext): string | undefined {
  return buildSystemPrompt(context.systemPrompt);
}

function isToolDefinition(value: unknown): value is ToolDefinition {
  return !!value && typeof value === "object" &&
    typeof (value as { name?: unknown }).name === "string" &&
    typeof (value as { description?: unknown }).description === "string" &&
    "inputSchema" in value;
}

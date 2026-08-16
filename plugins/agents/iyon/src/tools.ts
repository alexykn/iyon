import { executeTool, type AnyTool, type ToolExecutionHooks } from "@iyon/runtime";
import type { JsonValue, MessageId, ToolCallId, ToolContext, ToolDefinition, ToolResult, TurnId } from "@iyon/sdk";
import type { AgentModelTurnResult } from "./turn.ts";
import type { AgentPolicyContext } from "./context.ts";

export const MAX_TOOL_CALLS_PER_MODEL_TURN = 16;

export interface AgentToolContext extends AgentPolicyContext {
  readonly turnId?: TurnId;
  readonly messageId?: MessageId;
  readonly workspace?: ToolContext["workspace"];
  readonly cwd?: string;
  readonly signal?: AbortSignal;
  readonly approval?: ToolContext["approval"];
  readonly hooks?: {
    readonly before?: (tool: ToolDefinition, args: JsonValue) => void | JsonValue | Promise<void | JsonValue>;
    readonly after?: (tool: ToolDefinition, result: ToolResult) => void | ToolResult | Promise<void | ToolResult>;
  };
}

export interface ToolExecutionSummary {
  readonly completed: boolean;
  readonly results: readonly ToolResult[];
}

export async function executeRequestedTools(context: AgentToolContext, result: AgentModelTurnResult): Promise<ToolExecutionSummary> {
  const totalCalls = result.toolCalls.length + result.invalidToolCalls.length;
  if (totalCalls > MAX_TOOL_CALLS_PER_MODEL_TURN) {
    throw new Error(`model requested ${totalCalls} tool calls, maximum is ${MAX_TOOL_CALLS_PER_MODEL_TURN}`);
  }

  const sessionId = String(context.session.snapshot().sessionId) as never;
  const results: ToolResult[] = [];
  for (const invalid of result.invalidToolCalls) {
    if (context.signal?.aborted) return { completed: false, results };
    const argumentsValue = parseArguments(invalid.argumentsText);
    const request = {
      sessionId,
      turnId: result.turnId,
      messageId: context.messageId ?? 0 as never,
      toolCallId: (invalid.id ?? `invalid-${invalid.contentIndex}`) as ToolCallId,
      toolName: invalid.name ?? "<invalid>",
      arguments: argumentsValue,
    };
    let executed;
    try {
      executed = await executeTool(context.session, undefined, request, runtimeOptions(context));
    } catch (error) {
      if (context.signal?.aborted) return { completed: false, results };
      throw error;
    }
    results.push(executed.result);
  }

  for (const call of result.toolCalls) {
    if (context.signal?.aborted) return { completed: false, results };
    const tool = findTool(context, call.name);
    const request = {
      sessionId,
      turnId: result.turnId,
      messageId: context.messageId ?? 0 as never,
      toolCallId: call.id,
      toolName: call.name,
      arguments: call.arguments as JsonValue,
    };
    let executed;
    try {
      executed = await executeTool(context.session, tool as AnyTool | undefined, request, runtimeOptions(context, tool));
    } catch (error) {
      if (context.signal?.aborted) return { completed: false, results };
      throw error;
    }
    results.push(executed.result);
  }

  return { completed: !context.signal?.aborted, results };
}

function runtimeOptions(context: AgentToolContext, tool?: ToolDefinition) {
  const hooks: ToolExecutionHooks | undefined = tool && context.hooks === undefined ? undefined : tool === undefined || context.hooks === undefined ? undefined : {
    before: (toolContext, args) => context.hooks?.before?.(tool, args),
    after: (toolContext, value) => context.hooks?.after?.(tool, value),
  };
  return {
    signal: context.signal,
    cwd: context.cwd,
    workspace: context.workspace,
    approval: context.approval,
    updates: context.updates,
    hooks,
  };
}

function findTool(context: AgentToolContext, name: string): ToolDefinition | undefined {
  return context.tools?.list().map((entry) => entry.value).find((value): value is ToolDefinition => isTool(value) && value.name === name);
}

function isTool(value: unknown): value is ToolDefinition {
  return !!value && typeof value === "object" && typeof (value as { name?: unknown }).name === "string" && typeof (value as { execute?: unknown }).execute === "function";
}

function parseArguments(value: string): JsonValue {
  if (value.length === 0) return {};
  try {
    const parsed: unknown = JSON.parse(value);
    return isJsonValue(parsed) ? parsed : {};
  } catch {
    return {};
  }
}

function isJsonValue(value: unknown): value is JsonValue {
  if (value === null || typeof value === "string" || typeof value === "number" || typeof value === "boolean") return true;
  if (Array.isArray(value)) return value.every(isJsonValue);
  if (typeof value !== "object") return false;
  return Object.values(value).every(isJsonValue);
}

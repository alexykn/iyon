import type { ApprovalRequirement, JsonValue, MessageId, ToolCallId, ToolDefinition, ToolResult, ToolUpdateSink, TurnId, WorkspaceHandle } from "@iyon/sdk";
import type { AgentModelTurnResult } from "./turn.ts";
import type { AgentPolicyContext } from "./context.ts";

export const MAX_TOOL_CALLS_PER_MODEL_TURN = 16;

export interface AgentToolContext extends AgentPolicyContext {
  readonly turnId?: TurnId;
  readonly messageId?: MessageId;
  readonly workspace?: WorkspaceHandle;
  readonly cwd?: string;
  readonly signal?: AbortSignal;
  readonly approval?: (state: import("@iyon/sdk").ApprovalState) => Promise<boolean>;
  readonly hooks?: {
    readonly before?: (tool: ToolDefinition, args: JsonValue) => void | JsonValue | Promise<void | JsonValue>;
    readonly after?: (tool: ToolDefinition, result: ToolResult) => void | ToolResult | Promise<void | ToolResult>;
  };
}

export interface ToolExecutionSummary {
  readonly completed: boolean;
  readonly results: readonly ToolResult[];
}

interface PublicToolExecution {
  prepared(argumentsValue: JsonValue): void;
  start(): void;
  requestApproval(requirement?: ApprovalRequirement): import("@iyon/sdk").ApprovalState | null;
  approve(approvalId: number): void;
  reject(approvalId: number, reason?: string): void;
  finish(result: ToolResult): void;
  fail(error: string): void;
  cancel(reason?: string): void;
}

export async function executeRequestedTools(context: AgentToolContext, result: AgentModelTurnResult): Promise<ToolExecutionSummary> {
  const totalCalls = result.toolCalls.length + result.invalidToolCalls.length;
  if (totalCalls > MAX_TOOL_CALLS_PER_MODEL_TURN) throw new Error(`model requested ${totalCalls} tool calls, maximum is ${MAX_TOOL_CALLS_PER_MODEL_TURN}`);

  const results: ToolResult[] = [];
  for (const invalid of result.invalidToolCalls) {
    const callId = (invalid.id ?? `invalid-${invalid.contentIndex}`) as ToolCallId;
    const toolName = invalid.name ?? "<invalid>";
    const execution = prepare(context, result.turnId, callId, toolName, parseArguments(invalid.argumentsText));
    execution.prepared(parseArguments(invalid.argumentsText));
    execution.start();
    const invalidResult = errorResult(toolName, `Invalid tool call: ${invalid.reason}`);
    execution.finish(invalidResult);
    results.push(invalidResult);
  }

  for (const call of result.toolCalls) {
    const tool = findTool(context, call.name);
    const execution = prepare(context, result.turnId, call.id, call.name, call.arguments);
    execution.prepared(call.arguments);
    execution.start();
    const requirement = approvalRequirement(tool);
    const approval = execution.requestApproval(requirement);
    if (approval) {
      const approved = await context.approval?.(approval) ?? true;
      if (!approved) {
        execution.reject(approval.id, "tool approval rejected");
        results.push(errorResult(call.name, "tool approval rejected"));
        continue;
      }
      execution.approve(approval.id);
    }
    if (context.signal?.aborted) {
      execution.cancel("tool cancelled");
      return { completed: false, results };
    }
    if (!tool) {
      const unknown = errorResult(call.name, `Unknown tool: ${call.name}`);
      execution.finish(unknown);
      results.push(unknown);
      continue;
    }
    try {
      const before = await context.hooks?.before?.(tool, call.arguments);
      const argumentsValue = before !== undefined && typeof before === "object" ? before as JsonValue : call.arguments;
      const toolResult = await tool.execute(createToolContext(context, call.id, result.turnId), argumentsValue);
      const after = await context.hooks?.after?.(tool, toolResult);
      const finalResult = after && typeof after === "object" && "content" in after ? after : toolResult;
      execution.finish(finalResult);
      results.push(finalResult);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      execution.fail(message);
      results.push(errorResult(call.name, message));
    }
  }
  return { completed: true, results };
}

function prepare(context: AgentToolContext, turnId: TurnId, toolCallId: ToolCallId, toolName: string, argumentsValue: JsonValue): PublicToolExecution {
  return context.session.prepareToolExecution({
    sessionId: String(context.session.snapshot().sessionId) as never,
    turnId,
    messageId: context.messageId ?? 0 as never,
    toolCallId,
    toolName,
    arguments: argumentsValue,
  }) as unknown as PublicToolExecution;
}

function findTool(context: AgentToolContext, name: string): ToolDefinition | undefined {
  return context.tools?.list().map((entry) => entry.value).find((value): value is ToolDefinition => isTool(value) && value.name === name);
}

function isTool(value: unknown): value is ToolDefinition {
  return !!value && typeof value === "object" && typeof (value as { name?: unknown }).name === "string" && typeof (value as { execute?: unknown }).execute === "function";
}

function createToolContext(context: AgentToolContext, toolCallId: ToolCallId, turnId: TurnId): import("@iyon/sdk").ToolContext {
  const updates: ToolUpdateSink = { send: async () => undefined };
  return {
    sessionId: String(context.session.snapshot().sessionId) as never,
    turnId,
    messageId: context.messageId ?? 0 as never,
    toolCallId,
    cwd: context.cwd ?? process.cwd(),
    workspace: context.workspace ?? {},
    signal: context.signal ?? new AbortController().signal,
    updates,
    update: (update) => updates.send(update),
    approval: context.approval,
  };
}

function approvalRequirement(tool: ToolDefinition | undefined): ApprovalRequirement {
  const policy = tool?.approval ?? tool?.execution?.approval ?? tool?.metadata?.approval;
  if (policy === "alwaysAsk") return { type: "required" };
  if (policy && typeof policy === "object") return policy;
  return { type: "notRequired" };
}

function errorResult(toolName: string, text: string): ToolResult {
  return { content: [{ type: "text", text }], details: { toolName }, isError: true, toolName };
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

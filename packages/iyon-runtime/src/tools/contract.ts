import type { ApprovalRequirement, JsonValue, Tool, ToolCall, ToolCallId, ToolContext, ToolUpdateSink, ToolResult as SdkToolResult } from "@iyon/sdk";

export type { Tool, ToolCall, ToolContext, ToolUpdateSink } from "@iyon/sdk";
export type ToolResult<TValue = unknown> = SdkToolResult & {
  readonly value?: TValue;
  readonly toolCallId?: ToolCallId;
  readonly toolName?: string;
  readonly text?: string;
  readonly state?: ToolLifecycleState;
};

import type { ToolLifecycleState } from "@iyon/sdk";

export interface ToolExecutionHooks {
  readonly before?: (context: ToolContext, args: JsonValue) => void | JsonValue | Promise<void | JsonValue>;
  readonly after?: (context: ToolContext, result: ToolResult) => void | ToolResult | Promise<void | ToolResult>;
}

export interface ToolLifecycleOptions {
  readonly hooks?: ToolExecutionHooks;
  readonly workspace?: ToolContext["workspace"];
  readonly cwd?: string;
  readonly signal?: AbortSignal;
  readonly approval?: ToolContext["approval"];
  readonly updates?: ToolUpdateSink;
  readonly policy?: { approval(toolName: string, args: JsonValue, base: ApprovalRequirement): ApprovalRequirement };
}

export type AnyTool = Tool<JsonValue, unknown>;

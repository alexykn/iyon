import type { ApprovalRequirement, JsonValue } from "@iyon/sdk";

export interface ToolApprovalPolicyContribution {
  readonly approval: (toolName: string, args: JsonValue, base: ApprovalRequirement) => ApprovalRequirement;
}

export function createToolApprovalPolicy(approval: ToolApprovalPolicyContribution["approval"]): ToolApprovalPolicyContribution { return { approval }; }

export function bashCommandUsesSudo(args: JsonValue): boolean {
  if (!args || typeof args !== "object" || Array.isArray(args)) return false;
  const command = (args as { command?: unknown }).command;
  if (typeof command !== "string") return false;
  return command.split(/[\s;&|()]+/).some((token) => token === "sudo");
}

export const bashApprovalPolicy: ToolApprovalPolicyContribution = createToolApprovalPolicy((toolName, args, base) => toolName === "bash" && bashCommandUsesSudo(args) ? { type: "required", reason: "bash command uses sudo" } : base);

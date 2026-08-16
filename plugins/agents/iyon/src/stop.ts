import type { StopReason } from "iyon:api";

export type StopAction = "executeTools" | "finish";

export function classifyStopReason(stopReason: StopReason, hasToolCalls: boolean): StopAction {
  if (stopReason === "toolUse" && hasToolCalls) return "executeTools";
  if (stopReason === "toolUse") throw new Error("provider returned toolUse without tool calls");
  if ((stopReason === "stop" || stopReason === "length") && !hasToolCalls) return "finish";
  if (stopReason === "stop" || stopReason === "length") throw new Error(`provider returned ${stopReason} with tool calls`);
  throw new Error(`model stream ended with ${stopReason}`);
}

import type { StopReason } from "iyon:api";
import type { AgentModelTurnResult } from "./turn.ts";

export function hasRequestedCalls(result: AgentModelTurnResult): boolean {
  return result.toolCalls.length > 0 || result.invalidToolCalls.length > 0;
}

export function shouldContinue(stopReason: StopReason, result: AgentModelTurnResult, hasPendingSteering: boolean): boolean {
  if (hasRequestedCalls(result)) return true;
  if (stopReason === "stop" || stopReason === "length") return hasPendingSteering;
  return false;
}

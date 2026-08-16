import type { ModelParams, ReasoningLevel } from "iyon:api";

export function selectReasoningEffort(effort: ReasoningLevel | undefined): ModelParams {
  return effort === undefined ? {} : { reasoning: effort };
}

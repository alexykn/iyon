import type { ModelApi } from "iyon:api";
import type { KernelSession, ModelTurnResult } from "@iyon/sdk";
import type { CustomAgentContext } from "./custom-agent.ts";

export async function runIndependentSubagent(
  parent: CustomAgentContext,
  createAgent: (context: CustomAgentContext) => { run(): Promise<ModelTurnResult> },
  childModel: ModelApi,
): Promise<{ readonly childSession: KernelSession; readonly result: ModelTurnResult }> {
  const core = await import("iyon:core");
  const childSession = new core.KernelSession() as unknown as KernelSession;
  const result = await createAgent({ ...parent, session: childSession, model: childModel }).run();
  return { childSession, result };
}

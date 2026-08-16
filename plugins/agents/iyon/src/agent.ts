import type { ModelApi } from "iyon:api";
import type { KernelSession, MessageId, ReasoningLevel, WorkspaceHandle } from "@iyon/sdk";
import { buildModelRequest } from "./request.ts";
import { runProviderTurn, type AgentModelTurnResult } from "./turn.ts";
import { classifyStopReason } from "./stop.ts";
import { executeRequestedTools, type AgentToolContext } from "./tools.ts";
import { drainSteering, injectSteeredMessages, type SteeringQueue } from "./steering.ts";
import { hasRequestedCalls, shouldContinue } from "./continuation.ts";
import type { PublicToolRegistry } from "./context.ts";

export interface AgentContext extends AgentToolContext {
  readonly model: ModelApi;
  readonly signal: AbortSignal;
  readonly session: KernelSession;
  readonly tools?: PublicToolRegistry;
  readonly systemPrompt?: string;
  readonly reasoningEffort?: ReasoningLevel;
  readonly workspace?: WorkspaceHandle;
  readonly steering?: SteeringQueue;
}

export interface Agent {
  run(context?: AgentContext): Promise<void>;
}

export class IyonAgent implements Agent {
  constructor(private readonly context: AgentContext) {}

  async run(context = this.context): Promise<void> {
    while (!context.signal.aborted) {
      const steers = drainSteering(context.session, context.steering);
      injectSteeredMessages(context.session, steers);
      if (context.session.snapshot().entries.every((entry) => entry.kind !== "message")) return;
      const request = buildModelRequest(context);
      const result = await runProviderTurn(context, request);
      if (result.cancelled || context.signal.aborted) return;

      const action = classifyStopReason(result.stopReason, hasRequestedCalls(result));
      if (action === "executeTools") {
        const toolExecution = await executeRequestedTools({ ...context, messageId: assistantMessageId(result) }, result);
        if (!toolExecution.completed) return;
        continue;
      }

      const pendingSteering = drainSteering(context.session, context.steering);
      if (shouldContinue(result.stopReason, result, pendingSteering.length > 0)) {
        injectSteeredMessages(context.session, pendingSteering);
        continue;
      }
      return;
    }
  }
}

function assistantMessageId(result: AgentModelTurnResult): MessageId {
  return result.assistantMessage.id;
}

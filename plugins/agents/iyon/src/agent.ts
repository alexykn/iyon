import type { ModelApi } from "iyon:api";
import type { KernelSession, MessageId, ReasoningLevel, WorkspaceHandle } from "@iyon/sdk";
import { buildModelRequest } from "./request.ts";
import { runProviderTurn, type AgentModelTurnResult } from "./turn.ts";
import { classifyStopReason } from "./stop.ts";
import { executeRequestedTools, type AgentToolContext } from "./tools.ts";
import { drainPrompts, drainSteering, injectSteeredMessages, type SteeringQueue } from "./steering.ts";
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
  cancel(): void;
  setReasoningEffort(level: ReasoningLevel): void;
}

export class IyonAgent implements Agent {
  private readonly lifetimeContext: AgentContext;
  private activeController: AbortController | undefined;
  private reasoningEffort: ReasoningLevel | undefined;

  constructor(context: AgentContext) {
    this.lifetimeContext = context;
    this.reasoningEffort = context.reasoningEffort;
  }

  cancel(): void {
    this.activeController?.abort();
  }

  setReasoningEffort(level: ReasoningLevel): void {
    this.reasoningEffort = level;
  }

  async run(context = this.lifetimeContext): Promise<void> {
    if (this.activeController !== undefined) throw new Error("agent run is already active");
    const activeController = new AbortController();
    this.activeController = activeController;
    const linked = linkSignals(context.signal, activeController.signal);
    const runContext = { ...context, signal: linked.signal, reasoningEffort: this.reasoningEffort };

    try {
      // A single agent run may contain several model/tool cycles. Once the
      // model has requested a tool, keep new steering messages queued until
      // the run produces its final non-tool response. Draining at the top of
      // every loop made a message submitted during `read`/`bash` appear in
      // history as soon as that tool returned, even though the agent was
      // still working.
      let toolCycleStarted = false;
      while (!runContext.signal.aborted) {
        const queued = drainPrompts(runContext.session);
        if (!toolCycleStarted) {
          drainSteering(runContext.session, runContext.steering).forEach((message) => queued.push(message));
        }
        injectSteeredMessages(runContext.session, queued);
        if (runContext.session.snapshot().entries.every((entry) => entry.kind !== "message")) return;
        const request = buildModelRequest(runContext);
        const result = await runProviderTurn(runContext, request);
        if (result.cancelled || runContext.signal.aborted) return;

        const action = classifyStopReason(result.stopReason, hasRequestedCalls(result));
        if (action === "executeTools") {
          toolCycleStarted = true;
          const toolExecution = await executeRequestedTools({ ...runContext, messageId: assistantMessageId(result) }, result);
          if (!toolExecution.completed) return;
          continue;
        }

        // At this point the response completed without requesting another
        // tool. The tool cycle, if any, is over, so deliver queued messages
        // at the agent-run boundary and let the next response address them.
        const pending = drainPrompts(runContext.session);
        drainSteering(runContext.session, runContext.steering).forEach((message) => pending.push(message));
        if (shouldContinue(result.stopReason, result, pending.length > 0)) {
          injectSteeredMessages(runContext.session, pending);
          continue;
        }
        return;
      }
    } finally {
      linked.dispose();
      if (this.activeController === activeController) this.activeController = undefined;
    }
  }
}

function assistantMessageId(result: AgentModelTurnResult): MessageId {
  return result.assistantMessage.id;
}

function linkSignals(lifetime: AbortSignal, active: AbortSignal): { readonly signal: AbortSignal; dispose(): void } {
  const controller = new AbortController();
  const abort = (source: AbortSignal) => {
    if (!controller.signal.aborted) controller.abort(source.reason);
  };
  const onLifetimeAbort = () => abort(lifetime);
  const onActiveAbort = () => abort(active);
  if (lifetime.aborted) abort(lifetime);
  if (active.aborted) abort(active);
  lifetime.addEventListener("abort", onLifetimeAbort, { once: true });
  active.addEventListener("abort", onActiveAbort, { once: true });
  return {
    signal: controller.signal,
    dispose() {
      lifetime.removeEventListener("abort", onLifetimeAbort);
      active.removeEventListener("abort", onActiveAbort);
    },
  };
}

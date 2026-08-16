import type { ModelApi, ModelRequest, ModelStreamEvent } from "iyon:api";
import type { KernelSession, ModelTurnResult } from "@iyon/sdk";
import { StreamAssembler, type InvalidToolCall } from "./stream.ts";

export interface ProviderTurnContext {
  readonly session: KernelSession;
  readonly model: ModelApi;
  readonly signal?: AbortSignal;
}

export interface AgentModelTurnResult extends ModelTurnResult {
  readonly invalidToolCalls: readonly InvalidToolCall[];
}

interface PublicModelTurn {
  push(event: ModelStreamEvent, signal?: AbortSignal): Promise<void>;
  finish(): Promise<ModelTurnResult>;
  fail(error: { readonly kind: "provider"; readonly message: string } | string): Promise<void>;
  cancel(): Promise<ModelTurnResult>;
}

export async function runProviderTurn(context: ProviderTurnContext, request: ModelRequest): Promise<AgentModelTurnResult> {
  const turn = context.session.beginModelTurn({ request }) as unknown as PublicModelTurn;
  const assembler = new StreamAssembler();
  try {
    const stream = await context.model.stream(request, { signal: context.signal });
    for await (const event of stream) {
      assembler.observe(event);
      if (context.signal?.aborted) return withInvalidCalls(await turn.cancel(), assembler);
      if (event.type === "done" && assembler.invalidCalls().length > 0) return withInvalidCalls(await turn.cancel(), assembler);
      await turn.push(event, context.signal);
    }
    return withInvalidCalls(await turn.finish(), assembler);
  } catch (error) {
    if (context.signal?.aborted) return withInvalidCalls(await turn.cancel(), assembler);
    if (assembler.invalidCalls().length > 0) return withInvalidCalls(await turn.cancel(), assembler);
    const message = error instanceof Error ? error.message : String(error);
    await turn.fail({ kind: "provider", message });
    throw new Error(`provider stream failed: ${message}`, { cause: error });
  }
}

function withInvalidCalls(result: ModelTurnResult, assembler: StreamAssembler): AgentModelTurnResult {
  return { ...result, invalidToolCalls: assembler.invalidCalls() };
}

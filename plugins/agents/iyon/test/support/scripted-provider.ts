import type { ModelApi, ModelRequest, ModelStreamEvent } from "iyon:api";

export type ScriptedTurn =
  | readonly ModelStreamEvent[]
  | { readonly waitForAbort: true };

export class ScriptedProvider implements ModelApi {
  readonly requests: ModelRequest[] = [];
  private readonly turns: ScriptedTurn[] = [];

  enqueue(...turns: ScriptedTurn[]): void {
    this.turns.push(...turns);
  }

  async *stream(request: ModelRequest, options: { readonly signal?: AbortSignal } = {}): AsyncIterable<ModelStreamEvent> {
    this.requests.push(structuredClone(request));
    const turn = this.turns.shift();
    if (turn === undefined) throw new Error("scripted provider has no queued turn");
    if (isAbortTurn(turn)) {
      await waitForAbort(options.signal);
      yield { type: "done", stopReason: "aborted" };
      return;
    }
    for (const event of turn) {
      if (options.signal?.aborted) {
        yield { type: "done", stopReason: "aborted" };
        return;
      }
      yield event;
    }
  }
}

function isAbortTurn(turn: ScriptedTurn): turn is { readonly waitForAbort: true } {
  return !Array.isArray(turn) && "waitForAbort" in turn && turn.waitForAbort === true;
}

async function waitForAbort(signal?: AbortSignal): Promise<void> {
  if (signal?.aborted) return;
  await new Promise<void>((resolve) => signal?.addEventListener("abort", () => resolve(), { once: true }));
}

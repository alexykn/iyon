import type { ModelRequest, ModelStreamEvent } from "iyon:api";
import type { KernelSession as KernelSessionContract, ModelTurnResult, Tool } from "@iyon/sdk";
import { ScriptedProvider, type ScriptedTurn } from "./scripted-provider.ts";

interface CancellableModelTurn {
  push(event: ModelStreamEvent, signal?: AbortSignal): Promise<void>;
  finish(): Promise<ModelTurnResult>;
  cancel(): Promise<ModelTurnResult>;
}

export class SessionDriver {
  readonly provider = new ScriptedProvider();
  readonly session: KernelSessionContract;
  readonly tools = new Map<string, Tool>();

  private constructor(private readonly sessionConstructor: new (options?: { id?: number; eventCapacity?: number }) => KernelSessionContract, id = 1) {
    this.session = new sessionConstructor({ id });
  }

  static async create(id = 1): Promise<SessionDriver> {
    const core = await import("iyon:core") as unknown as { KernelSession: new (options?: { id?: number; eventCapacity?: number }) => KernelSessionContract };
    return new SessionDriver(core.KernelSession, id);
  }

  enqueue(...turns: ScriptedTurn[]): void {
    this.provider.enqueue(...turns);
  }

  async runTurn(request: ModelRequest, signal?: AbortSignal): Promise<ModelTurnResult> {
    // The public turn handle owns cancellation; the native JSON boundary only
    // receives the serializable request.
    const turn = this.session.beginModelTurn({ request }) as unknown as CancellableModelTurn;
    try {
      for await (const event of this.provider.stream(request, { signal })) {
        if (signal?.aborted) return turn.cancel();
        await turn.push(event, signal);
      }
    } catch (error) {
      if (!signal?.aborted) throw error;
      return turn.cancel();
    }
    return turn.finish();
  }

  async pushTurnEvents(turn: CancellableModelTurn, events: Iterable<ModelStreamEvent>, signal?: AbortSignal): Promise<ModelTurnResult> {
    for (const event of events) await turn.push(event, signal);
    return turn.finish();
  }

  snapshot() {
    return this.session.snapshot();
  }

  close(): void {
    this.session.close();
  }
}

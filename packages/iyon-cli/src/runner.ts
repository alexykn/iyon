import type { AgentSession } from "@iyon/runtime";
import type { CoreEvent } from "@iyon/sdk";

export interface CoreEventSource { nextEvent(signal?: AbortSignal): Promise<CoreEvent | null>; close?(): void; }
export interface CoreEventBridge { readonly done: Promise<void>; close(): void; }

export interface RunnableAgent { run(): Promise<void>; cancel?(): Promise<void> | void; }
export interface RunnableApp { start(): Promise<void>; stop(): Promise<void>; startBackendBridge?(source: CoreEventSource): CoreEventBridge; }
export interface RunnableApp { run?(signal?: AbortSignal): Promise<void>; }

export interface RunnerOptions { readonly app: RunnableApp; readonly agent: RunnableAgent; readonly session: AgentSession; readonly signal?: AbortSignal; }

export async function runSelectedApp(options: RunnerOptions): Promise<void> {
  const controller = new AbortController();
  const onAbort = () => { controller.abort(); void options.agent.cancel?.(); };
  options.signal?.addEventListener("abort", onAbort, { once: true });
  const bridge = options.app.startBackendBridge?.(options.session);
  try {
    if (options.app.run) await options.app.run(controller.signal);
    else await options.app.start();
  } finally {
    bridge?.close();
    if (bridge) await bridge.done;
    await options.app.stop();
    options.signal?.removeEventListener("abort", onAbort);
  }
}

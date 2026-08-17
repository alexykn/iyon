import type { AgentSession } from "@iyon/runtime";
import type { CoreEvent } from "@iyon/sdk";
import { isExpectedCleanupError } from "./cleanup.ts";

export interface CoreEventSource { nextEvent(signal?: AbortSignal): Promise<CoreEvent | null>; close?(): void; }
export interface CoreEventBridge { readonly done: Promise<void>; close(): void; }

export interface RunnableAgent { run(): Promise<void>; cancel?(): Promise<void> | void; }
export interface RunnableApp { start(): Promise<void>; stop(): Promise<void>; startBackendBridge?(source: CoreEventSource): CoreEventBridge; }
export interface RunnableApp { run?(signal?: AbortSignal): Promise<void>; }

export interface RunnerOptions { readonly app: RunnableApp; readonly agent: RunnableAgent; readonly session: AgentSession; readonly signal?: AbortSignal; }

let processSignalAction: (() => Promise<void>) | undefined;

export function requestProcessSignal(): Promise<void> | undefined {
  return processSignalAction?.();
}

export async function runSelectedApp(options: RunnerOptions): Promise<void> {
  const controller = new AbortController();
  let cancellation: Promise<void> | undefined;
  const onAbort = () => {
    controller.abort();
    cancellation = Promise.resolve(options.agent.cancel?.());
  };
  options.signal?.addEventListener("abort", onAbort, { once: true });
  if (options.signal?.aborted) onAbort();
  let bridge: CoreEventBridge | undefined;
  let signalAction: Promise<void> | undefined;
  let signalError: unknown;
  const requestAction = () => {
    if (signalAction !== undefined) return signalAction;
    const app = options.app as RunnableApp & { handleAction?: (action: unknown) => Promise<void> };
    if (app.handleAction === undefined) {
      onAbort();
      signalAction = Promise.resolve();
      return signalAction;
    }
    signalAction = Promise.resolve(app.handleAction({ type: "ctrlC" })).catch((error: unknown) => {
      signalError = error;
    });
    return signalAction;
  };
  processSignalAction = requestAction;
  try {
    bridge = options.app.startBackendBridge?.(options.session);
    if (options.app.run) await options.app.run(controller.signal);
    else await options.app.start();
  } finally {
    if (signalAction !== undefined) await signalAction;
    bridge?.close();
    if (bridge) await bridge.done;
    try { await options.app.stop(); } catch (error) {
      if (!isExpectedCleanupError(error)) throw error;
    }
    if (cancellation) await cancellation;
    options.signal?.removeEventListener("abort", onAbort);
    if (processSignalAction === requestAction) processSignalAction = undefined;
    if (signalError !== undefined) throw signalError;
  }
}

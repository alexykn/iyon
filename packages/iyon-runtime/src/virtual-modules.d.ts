declare module "iyon:api" {
  export const apiSmoke: "iyon:api/t1";
  export const nativeVersion: () => string;
  export const echoJson: (value: import("./native.ts").JsonValue) => import("./native.ts").JsonValue;
  export const echoString: (value: string) => string;
  export const echoBuffer: (value: Buffer) => Buffer;
}

declare module "iyon:core" {
  export const AgentSession: any;
  export const IyonNativeError: any;
  export const KernelSession: new (...args: any[]) => {
    snapshot(): SessionSnapshot;
    appendMessage(...args: any[]): number;
    [key: string]: any;
  };
  export const ModelTurn: any;
  export const ToolExecution: any;
  export const asIyonError: any;
  export const isCancelledError: any;
  export type SessionEntry = { readonly role?: string };
  export type SessionSnapshot = {
    readonly sessionId?: number;
    readonly entries: readonly SessionEntry[];
  };
  export type ToolResult = any;
  export const coreSmoke: "iyon:core/t1";
  export function runWithAbortSignal<T>(
    signal: AbortSignal,
    operation: { run(): Promise<T>; cancel(): void },
  ): Promise<T>;
  export function cancellationOperation(ms: number): {
    run(): Promise<string>;
    cancel(): void;
  };
  export const asyncSleep: (ms: number) => Promise<string>;
  export const CancellationProbe: new () => {
    run(ms: number): Promise<string>;
    cancel(): void;
  };
  export const NativeCounter: new () => import("./native.ts").NativeCounterContract;
  export const EventQueueProbe: new () => import("./native.ts").EventQueueProbeContract;
  export const nativeCounterStats: () => import("./native.ts").NativeCounterStats;
  export const resetNativeCounterStats: () => void;
}

declare module "iyon:tui" {
  export const tuiSmoke: "iyon:tui/t1";
  export class TuiError extends Error {
    readonly category: import("./tui/errors.ts").TuiErrorCategory;
    readonly nativeCode?: string;
    readonly context?: Readonly<Record<string, unknown>>;
  }
  export function asTuiError(error: unknown): TuiError;
  export function isTuiError(error: unknown): error is TuiError;
  export function isTuiCancelledError(error: unknown): boolean;
}

declare module "*.node" {
  const nativeAddon: import("./native.ts").NativeAddon;
  export default nativeAddon;
}

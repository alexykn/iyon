declare module "iyon:api" {
  export const apiSmoke: "iyon:api/t1";
  export const nativeVersion: () => string;
  export const echoJson: (value: import("./native.ts").JsonValue) => import("./native.ts").JsonValue;
  export const echoString: (value: string) => string;
  export const echoBuffer: (value: Buffer) => Buffer;
}

declare module "iyon:core" {
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
}

declare module "*.node" {
  const nativeAddon: import("./native.ts").NativeAddon;
  export default nativeAddon;
}

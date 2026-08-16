export type JsonPrimitive = null | boolean | number | string;
export type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };

export interface NativeCounterStats {
  live: number;
  finalized: number;
}

export interface CancellationProbeContract {
  run(ms: number): Promise<string>;
  cancel(): void;
}

export interface NativeCounterContract {
  increment(): number;
  value(): number;
}

export interface EventQueueProbeContract {
  send(event: JsonValue): Promise<void>;
  nextEvent(): Promise<JsonValue | null>;
  close(): void;
}

export interface NativeAddon {
  nativeVersion(): string;
  echoJson(value: JsonValue): JsonValue;
  echoString(value: string): string;
  echoBuffer(value: Buffer): Buffer;
  tuiSmoke(): string;
  asyncSleep(ms: number): Promise<string>;
  CancellationProbe: new () => CancellationProbeContract;
  NativeCounter: new () => NativeCounterContract;
  EventQueueProbe: new () => EventQueueProbeContract;
  nativeCounterStats(): NativeCounterStats;
  resetNativeCounterStats(): void;
}

// This is the one static addon seam. The stage script materializes this exact
// path before Bun typechecking, tests, or standalone compilation. A static
// require keeps the .node reachable to Bun's compiler for embedding.
export const native = require("../native/iyon-native.node") as NativeAddon;

export const {
  nativeVersion,
  echoJson,
  echoString,
  echoBuffer,
  tuiSmoke: nativeTuiSmoke,
  asyncSleep,
  CancellationProbe,
  NativeCounter,
  EventQueueProbe,
  nativeCounterStats,
  resetNativeCounterStats,
} = native;

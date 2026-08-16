export type JsonPrimitive = null | boolean | number | string;
export type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };

export interface NativeModelTurnContract {
  push(event: JsonValue): Promise<void>;
  pushMany(events: JsonValue[]): Promise<void>;
  finish(): Promise<JsonValue>;
  fail(error: JsonValue): Promise<void>;
  cancel(): Promise<JsonValue>;
}

export interface NativeToolExecutionContract {
  state(): string;
  events(): JsonValue[];
  prepared(argumentsValue: JsonValue): void;
  start(): void;
  requestApproval(requirement?: JsonValue): JsonValue | null;
  approve(approvalId: number): void;
  reject(approvalId: number, reason?: string): void;
  finish(result: JsonValue): void;
  fail(error: string): void;
  cancel(reason?: string): void;
}

export interface NativeKernelSessionContract {
  snapshot(): JsonValue;
  appendMessage(message: JsonValue): number;
  appendEntry(entry: JsonValue): void;
  nextEvent(): Promise<JsonValue | null>;
  beginModelTurn(options: JsonValue): NativeModelTurnContract;
  prepareToolExecution(request: JsonValue): NativeToolExecutionContract;
  enqueue(kind: string, text: string): void;
  dequeue(kind: string): string | null;
  queueSnapshot(): JsonValue;
  abort(): void;
  close(): void;
}

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
  KernelSession: new (options?: JsonValue) => NativeKernelSessionContract;
  nativeCounterStats(): NativeCounterStats;
  resetNativeCounterStats(): void;
  materializeView?(value: unknown): object;
  NativeHistory?: new () => { dispose(): void; layout(): object; push(view: object): void; pushStream(stream: object): void };
  NativeTextInput?: new (multiline?: boolean) => { dispose(): void; text(): string; cursorBytes(): number; setText(value: string): void; clear(): void; submitted(): string | null; setMultiline(enabled: boolean): void; isMultiline(): boolean };
  NativeTextStream?: new () => { dispose(): void; update(text: string): void; seal(): void; snapshot(): object };
  NativeComponent?: new () => { dispose(): void; id(): number; revision(): number };
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
  KernelSession,
  nativeCounterStats,
  resetNativeCounterStats,
} = native;

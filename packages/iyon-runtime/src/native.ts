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
  sendUpdate(update: JsonValue): void;
  finish(result: JsonValue): void;
  fail(error: string): void;
  cancel(reason?: string): void;
}

export interface NativeKernelSessionContract {
  snapshot(): JsonValue;
  appendMessage(message: JsonValue): number;
  deliverUserMessage(text: string): number;
  appendEntry(entry: JsonValue): void;
  nextEvent(): Promise<JsonValue | null>;
  nextEvents(max?: number): Promise<JsonValue[]>;
  beginModelTurn(options: JsonValue): NativeModelTurnContract;
  prepareToolExecution(request: JsonValue): NativeToolExecutionContract;
  enqueue(kind: string, text: string): number;
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

/** Private application/kernel native contract. It must not contain TUI APIs. */
export interface NativeCoreAddon {
  nativeVersion(): string;
  echoJson(value: JsonValue): JsonValue;
  echoString(value: string): string;
  echoBuffer(value: Buffer): Buffer;
  asyncSleep(ms: number): Promise<string>;
  CancellationProbe: new () => CancellationProbeContract;
  NativeCounter: new () => NativeCounterContract;
  EventQueueProbe: new () => EventQueueProbeContract;
  KernelSession: new (options?: JsonValue) => NativeKernelSessionContract;
  nativeCounterStats(): NativeCounterStats;
  resetNativeCounterStats(): void;
  credentialGet(service: string, account: string): string | undefined;
  credentialSet(service: string, account: string, secret: string): void;
  credentialDelete(service: string, account: string): void;
  credentialHas(service: string, account: string): boolean;
}

// This is the application/kernel native seam. TUI loading belongs to @iyon/tui.
export const native = require("../native/iyon-core-native.node") as NativeCoreAddon;

export const {
  nativeVersion,
  echoJson,
  echoString,
  echoBuffer,
  asyncSleep,
  CancellationProbe,
  NativeCounter,
  EventQueueProbe,
  KernelSession,
  nativeCounterStats,
  resetNativeCounterStats,
  credentialGet,
  credentialSet,
  credentialDelete,
  credentialHas,
} = native;

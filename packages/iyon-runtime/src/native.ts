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
  deliverUserMessage(text: string): number;
  appendEntry(entry: JsonValue): void;
  nextEvent(): Promise<JsonValue | null>;
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
  credentialGet(service: string, account: string): string | undefined;
  credentialSet(service: string, account: string, secret: string): void;
  credentialDelete(service: string, account: string): void;
  credentialHas(service: string, account: string): boolean;
  materializeView?(value: unknown): object;
  NativeHistory?: new () => { dispose(): void; layout(): object; setLayout(layout: object): void; isDetached(): boolean; push(view: object): number; freeze(unit: number, view: object): void; discardLive(unit: number): void; pushStream(stream: object): void; sealStream(stream: object): void };
  NativeTextInput?: new (multiline?: boolean) => { dispose(): void; text(): string; cursorBytes(): number; setText(value: string): void; clear(): void; submitted(): NativeTuiOutputContract; setMultiline(enabled: boolean): void; isMultiline(): boolean; componentId(): number | null };
  NativeTuiHost?: new (width?: number, height?: number, headless?: boolean) => NativeTuiHostContract;
  NativeTuiOutput?: new () => NativeTuiOutputContract;
  NativeTextStream?: new (options?: "markdown" | { readonly projector?: "markdown"; readonly presentation?: object }) => { dispose(): void; update(text: string): void; append(text: string, annotations?: readonly object[]): void; seal(): void; snapshot(): object };
  NativeMarkdownProjector?: new () => { dispose(): void; project(text: string, sealed?: boolean): object };
  NativePlainProjector?: new () => { dispose(): void; project(text: string): object };
  NativeViewSlot?: new (initial: object) => { dispose(): void; revision(): number; componentId(): number | null; setView(view: object): void; setAnimation(frames: object[], intervalMs: number): void; stopAnimation(view: object): void };
  NativeScrollPane?: new (initial: object) => { dispose(): void; componentId(): number | null; setContent(view: object): void; followEnd(): void };
}

export interface NativeTuiOutputContract { readonly output?: unknown; }

export interface NativeTuiHostContract {
  dispose(): void;
  exit(): void;
  history(): object;
  textInput(multiline?: boolean, border?: object): object;
  setTheme(theme: object): void;
  setHistory(history: object): void;
  exited(): boolean;
  bindKey(key: string, modifiers: readonly string[] | undefined, routeId: string): void;
  route(output: NativeTuiOutputContract, routeId: string): void;
  interceptPaste(input: object, routeId: string): void;
  render(view: object): void;
  dispatchKey(key: string, modifiers?: readonly string[]): void;
  dispatchPaste(text: string): void;
  forwardPaste(text: string): void;
  pollTerminal(): void;
  nextWakeMs(): number;
  nextOutput(): { route_id: string; payload?: string | null } | null;
  waitForOutput(): Promise<{ route_id: string; payload?: string | null } | null>;
  nextAction(): { action_id: string; payload?: string | null } | null;
  waitForAction(): Promise<{ action_id: string; payload?: string | null } | null>;
  screenRows(): string[];
  nativeHistoryRows(): string[];
  resize(width: number, height: number): void;
  advanceTime(milliseconds: number): void;
  createViewSlot(initial: object): object;
  scrollPane(initial: object): object;
  styleAt(row: number, column: number): object | null;
  cellXOfText(row: number, text: string): number | null;
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
  credentialGet,
  credentialSet,
  credentialDelete,
  credentialHas,
} = native;

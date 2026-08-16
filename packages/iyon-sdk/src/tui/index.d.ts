export type TuiErrorCategory =
  | "invalid-handle"
  | "disposed-handle"
  | "validation"
  | "terminal"
  | "runtime"
  | "projection"
  | "stream"
  | "cancelled";

export interface TuiError extends Error {
  readonly category: TuiErrorCategory;
  readonly nativeCode?: string;
  readonly context?: Readonly<Record<string, unknown>>;
}

export interface View {
  readonly kind: "view";
}

export interface TextContent {
  readonly kind: "text-content";
}

export interface Style {
  readonly kind: "style";
}

export interface NativeHandle {
  readonly kind: string;
  readonly id: number;
  readonly disposed: boolean;
  dispose(): Promise<void>;
}

export interface History extends NativeHandle {
  readonly kind: "history";
  layout(): Promise<HistoryLayout>;
}

export interface HistoryLayout {
  readonly padding: number;
  readonly gap: number;
}

export interface TextInput extends NativeHandle {
  readonly kind: "text-input";
  text(): Promise<string>;
  cursorBytes(): Promise<number>;
  setText(value: string): Promise<void>;
  clear(): Promise<void>;
  submitted(): Promise<string | null>;
  setMultiline(enabled: boolean): Promise<void>;
  isMultiline(): Promise<boolean>;
  view(): Promise<View>;
}

export interface TextStream extends NativeHandle {
  readonly kind: "text-stream";
  update(text: string): Promise<void>;
  seal(): Promise<void>;
  snapshot(): Promise<StreamSnapshot>;
}

export type StreamPane = TextStream;

export interface StreamSnapshot {
  readonly text: string;
  readonly revision: number;
  readonly sealed: boolean;
}

export interface Component extends NativeHandle {
  readonly kind: "component";
  view(): Promise<View>;
  capabilities(): Promise<ComponentCapabilities>;
}

export interface ComponentCapabilities {
  readonly focusable?: boolean;
  readonly modal?: boolean;
  readonly keys?: readonly string[];
  readonly paste?: boolean;
  readonly ticks?: boolean;
}

export interface Renderer {
  render(view: View, context?: RenderContext): View | Promise<View>;
}

export interface Projector {
  project(content: TextContent): TextContent | Promise<TextContent>;
}

export interface TextVisitor {
  visit(content: TextContent): void | Promise<void>;
}

export interface TextRewriter {
  rewrite(content: TextContent): TextContent | Promise<TextContent>;
}

export interface StreamingSource {
  snapshot(): StreamSnapshot | Promise<StreamSnapshot>;
  advance(): boolean | Promise<boolean>;
  seal(): void | Promise<void>;
  compact?(): void | Promise<void>;
}

export interface Scene {
  readonly history?: History;
  readonly body: View;
}

export interface TuiOpenOptions {
  readonly width?: number;
  readonly height?: number;
  readonly headless?: boolean;
  readonly signal?: AbortSignal;
}

export interface TerminalMetadata {
  readonly width: number;
  readonly height: number;
}

export interface TuiRuntime {
  readonly size: Promise<TerminalMetadata>;
  nextEvent(signal?: AbortSignal): Promise<TuiEvent>;
  render(scene: Scene, signal?: AbortSignal): Promise<void>;
  resize(width: number, height: number): Promise<void>;
  close(): Promise<void>;
}

export interface AppHarness extends TuiRuntime {
  pressKey(key: string, modifiers?: readonly string[]): void;
  paste(text: string): void;
  advance(ms: number): void;
  screenRows(): readonly string[];
  nativeHistoryRows(): readonly string[];
  styleAt(row: number, column: number): Readonly<Record<string, unknown>>;
  cellXOfText(row: number, text: string): number | null;
  exited(): boolean;
}

export interface KeyEvent {
  readonly type: "key";
  readonly key: string;
  readonly modifiers?: readonly string[];
}

export interface PasteEvent {
  readonly type: "paste";
  readonly text: string;
}

export interface ResizeEvent {
  readonly type: "resize";
  readonly width: number;
  readonly height: number;
}

export interface TerminateEvent {
  readonly type: "terminate";
  readonly reason?: string;
}

export type TuiEvent = KeyEvent | PasteEvent | ResizeEvent | TerminateEvent;
export interface RenderContext { readonly width: number; readonly height: number; }

export const tuiSmoke: "iyon:tui/t5";

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

export declare class View {
  readonly kind: "view";
  static text(value: string): View;
  static styledText(spans: readonly TextSpan[]): View;
  static spacer(rows: number): View;
  static diff(hunks: readonly DiffHunk[]): View;
  static horizontal(children: readonly View[] | ((builder: ChildrenBuilder) => void)): View;
  static vertical(children: readonly View[] | ((builder: ChildrenBuilder) => void)): View;
  static hanging(prefix: View, continuation: View, body: View): View;
  static grid(children: readonly View[]): View;
  bold(): View;
  dim(): View;
  italic(): View;
  underline(): View;
  reversed(): View;
  strikethrough(): View;
  padding(value: number | Insets): View;
  background(color: string): View;
  foreground(color: string): View;
  border(border: Readonly<Record<string, unknown>>): View;
  style(style: StyleSpec): View;
  styleState(key: string, value: string): View;
  container(): View;
  clampRows(maxRows: number): View;
  fitWidth(): View;
  fillWidth(): View;
  fitHeight(): View;
  fillHeight(): View;
  minWidth(value: number): View;
  maxWidth(value: number): View;
  minHeight(value: number): View;
  maxHeight(value: number): View;
  noWrap(): View;
}

export declare class ChildrenBuilder {
  child(view: View): this;
  childrenOf(views: readonly View[]): this;
  gap(value: number): this;
  fixed(size: number, view: View): this;
  flex(view: View): this;
}

export declare class Insets {
  static all(value: number): Insets;
  static vertical(value: number): Insets;
  static horizontal(value: number): Insets;
  static of(top: number, right: number, bottom: number, left: number): Insets;
}

export declare class StyleSpec {
  foreground(color: string): StyleSpec;
  background(color: string): StyleSpec;
  attribute(name: string, enabled?: boolean): StyleSpec;
  bold(): StyleSpec;
  dim(): StyleSpec;
  italic(): StyleSpec;
  underline(): StyleSpec;
  reversed(): StyleSpec;
  strikethrough(): StyleSpec;
  plain(): StyleSpec;
}

export declare const Style: {
  new(): StyleSpec;
  plain(): StyleSpec;
};

export declare class TextSpan {
  static plain(text: string): TextSpan;
  static styled(text: string, style: StyleSpec): TextSpan;
}

export type DiffLineKind = "context" | "addition" | "deletion";
export type DiffLineTermination = "lf" | "crlf" | "none";

export declare class DiffRange {
  readonly kind: "diff-range";
  readonly start: number;
  readonly lineCount: number;
  constructor(start: number, lineCount: number);
  isEmpty(): boolean;
  end(): number;
}

export declare class DiffLine {
  readonly kind: "diff-line";
  readonly lineKind: DiffLineKind;
  readonly text: string;
  readonly termination: DiffLineTermination;
  readonly oldLine?: number;
  readonly newLine?: number;
  constructor(lineKind: DiffLineKind, text: string, termination?: DiffLineTermination, coordinates?: { readonly oldLine?: number; readonly newLine?: number });
  static context(oldLine: number, newLine: number, text: string, termination?: DiffLineTermination): DiffLine;
  static addition(newLine: number, text: string, termination?: DiffLineTermination): DiffLine;
  static deletion(oldLine: number, text: string, termination?: DiffLineTermination): DiffLine;
}

export declare class DiffHunk {
  readonly kind: "diff-hunk";
  readonly oldRange: DiffRange;
  readonly newRange: DiffRange;
  readonly lines: readonly DiffLine[];
  constructor(oldRange: DiffRange, newRange: DiffRange, lines?: readonly DiffLine[]);
  validate(): void;
  render(): View;
}

export declare class DiffRenderer {
  render(hunks: DiffHunk | readonly DiffHunk[]): View;
  renderHunk(hunk: DiffHunk): View;
}

export declare class TextContent {
  readonly kind: "text-content";
  static plain(value: string): TextContent;
  static markdown(value: string): TextContent;
  static raw(value: string, origin?: TextOrigin): TextContent;
  readonly value: string;
  readonly origin: TextOrigin;
  text(): string;
  withOrigin(origin: TextOrigin): TextContent;
  render(): View;
}

export interface TextOrigin { readonly format: "plain" | "markdown"; readonly source?: string; }

export interface Style {
  readonly kind: "style";
}

export interface NativeHandle {
  readonly kind: string;
  readonly id: number;
  readonly disposed: boolean;
  dispose(): void;
}

export interface OutputHandle<T> {
  readonly kind: "output";
  readonly payload: T;
}

export declare class History implements NativeHandle {
  readonly id: number;
  readonly disposed: boolean;
  readonly kind: "history";
  constructor();
  dispose(): void;
  layout(): HistoryLayout;
  push(view: View): number;
  freeze(unit: number, view: View): void;
  pushStream(stream: TextStream): void;
  sealStream(stream: TextStream): void;
}

export interface HistoryLayout {
  readonly padding: number;
  readonly gap: number;
}

export declare class TextInput implements NativeHandle {
  readonly id: number;
  readonly disposed: boolean;
  readonly kind: "text-input";
  constructor(options?: { multiline?: boolean });
  dispose(): void;
  text(): string;
  cursorBytes(): number;
  setText(value: string): void;
  clear(): void;
  submitted(): OutputHandle<string>;
  setMultiline(enabled: boolean): void;
  isMultiline(): boolean;
  view(): View;
}

export interface StreamAnnotation { readonly namespace: string; readonly name: string; }

export interface TextStreamOptions {
  readonly projector?: "markdown";
  readonly presentation?: TextStreamPresentation;
  readonly pacing?: TextStreamPacing;
}

export interface TextStreamPresentation {
  readonly insets?: { readonly top?: number; readonly right?: number; readonly bottom?: number; readonly left?: number };
}

export interface TextStreamPacing {
  readonly tickIntervalMs?: number;
  readonly spring?: number;
  readonly minUnitsPerSecond?: number;
  readonly maxUnitsPerSecond?: number;
}

export declare class TextStream implements NativeHandle {
  readonly id: number;
  readonly disposed: boolean;
  readonly kind: "text-stream";
  constructor(options?: TextStreamOptions);
  dispose(): void;
  update(text: string): void;
  append(text: string, annotations?: readonly StreamAnnotation[]): void;
  seal(): void;
  snapshot(): StreamSnapshot;
}

export type StreamPane = TextStream;

export interface StreamSnapshot {
  readonly text: string;
  readonly revision: number;
  readonly sealed: boolean;
  readonly segments?: readonly StreamSegmentSnapshot[];
}

export interface StreamSegmentSnapshot {
  readonly annotations: readonly StreamAnnotation[];
  readonly text: string;
}

export declare class Component implements NativeHandle {
  readonly id: number;
  readonly disposed: boolean;
  readonly kind: "component";
  constructor();
  dispose(): void;
  view(): View;
  capabilities(): ComponentCapabilities;
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

export declare class Scene {
  readonly body: View;
  readonly history?: History;
  constructor(body: View, history?: History);
  static from(value: { readonly body: View; readonly history?: History }): Scene;
}

export interface SceneValue {
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
  readonly size: TerminalMetadata;
  nextEvent(signal?: AbortSignal): Promise<TuiEvent>;
  render(scene: Scene, signal?: AbortSignal): void;
  resize(width: number, height: number): void;
  close(): void;
  exit(): void;
}

export declare class Tui implements TuiRuntime {
  readonly size: TerminalMetadata;
  static open(options?: TuiOpenOptions): Promise<Tui>;
  nextEvent(signal?: AbortSignal): Promise<TuiEvent>;
  render(scene: SceneValue, signal?: AbortSignal): void;
  resize(width: number, height: number): void;
  close(): void;
  exit(): void;
}

export declare class AppHarness implements TuiRuntime {
  readonly size: TerminalMetadata;
  static open(options?: TuiOpenOptions): Promise<AppHarness>;
  nextEvent(signal?: AbortSignal): Promise<TuiEvent>;
  render(scene: SceneValue, signal?: AbortSignal): void;
  resize(width: number, height: number): void;
  close(): void;
  exit(): void;
  pressKey(key: string, modifiers?: readonly string[]): void;
  paste(text: string): void;
  advance(ms: number): void;
  screenRows(): readonly string[];
  nativeHistoryRows(): readonly string[];
  styleAt(row: number, column: number): Readonly<Record<string, unknown>>;
  cellXOfText(row: number, text: string): number | null;
  exited(): boolean;
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

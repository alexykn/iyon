import { native } from "../native.ts";
import { materializeView } from "./materialize.ts";
import { asTuiError, tuiError } from "./errors.ts";
import { requireNativeClass } from "./handles.ts";
import { Scene } from "./scene.ts";
import { History } from "./history.ts";
import { TextInput } from "./text-input.ts";
import { ViewSlot } from "./component.ts";
import { NativeScrollPane } from "./scroll-pane.ts";
import type {
  OutputHandle,
  ScrollPane,
  Scene as SceneContract,
  TerminalMetadata,
  TuiEvent,
  TuiOpenOptions,
  TuiRuntime,
} from "./types.ts";
import type { NativeTuiHostContract } from "../native.ts";

export class Tui implements TuiRuntime {
  private closed = false;
  private readonly host: NativeTuiHostContract;
  private readonly width: number;
  private readonly height: number;
  private currentScene?: Scene;

  private constructor(host: NativeTuiHostContract, width: number, height: number) {
    this.host = host;
    this.width = width;
    this.height = height;
  }

  static async open(options: TuiOpenOptions = {}): Promise<Tui> {
    if (options.signal?.aborted) throw tuiError("cancelled", "TUI open was cancelled");
    const width = options.width ?? 80;
    const height = options.height ?? 24;
    validateSize(width, height);
    const Host = requireNativeClass(native.NativeTuiHost, "NativeTuiHost");
    try {
      const tui = new Tui(new Host(width, height, options.headless ?? false), width, height);
      if (options.theme !== undefined) await tui.setTheme(options.theme);
      return tui;
    } catch (error) {
      throw asTuiError(error);
    }
  }

  get size(): Promise<TerminalMetadata> { return Promise.resolve({ width: this.width, height: this.height }); }

  async nextEvent(signal?: AbortSignal): Promise<TuiEvent> {
    if (signal?.aborted) throw tuiError("cancelled", "TUI event wait was cancelled");
    if (this.closed) return { type: "terminate", reason: "closed" };
    const output = await this.host.waitForOutput();
    if (signal?.aborted) throw tuiError("cancelled", "TUI event wait was cancelled");
    if (output === null) return { type: "terminate", reason: "closed" };
    return {
      type: "output",
      routeId: output.route_id,
      ...(output.payload === null || output.payload === undefined ? {} : { payload: output.payload }),
    };
  }

  /** Compatibility adapter for the pre-generic application harness. */
  async nextAction(signal?: AbortSignal): Promise<{ actionId: string; payload?: string } | null> {
    const event = await this.nextEvent(signal);
    if (event.type === "terminate") return null;
    if (event.type !== "output") return this.nextAction(signal);
    return { actionId: event.routeId, ...(event.payload === undefined ? {} : { payload: event.payload }) };
  }

  async render(scene: SceneContract, signal?: AbortSignal): Promise<void> {
    ensureSignal(signal);
    if (this.closed) throw tuiError("terminal", "TUI runtime is closed");
    const normalized = Scene.from(scene);
    if (normalized.history !== undefined) {
      const history = (normalized.history as unknown as { nativeObject(): object }).nativeObject() as { isDetached?: () => boolean };
      if (history.isDetached?.() === true) this.host.setHistory(history as object);
    }
    const lowered = materializeView(normalized.body);
    if (lowered === undefined) throw tuiError("runtime", "native View materialization is unavailable");
    this.host.render(lowered as object);
    this.currentScene = normalized;
  }

  createHistory(): History { return new History(this.host.history() as never); }

  createTextInput(options: { multiline?: boolean; border?: import("./ir.ts").BorderNode } = {}): TextInput {
    return new TextInput(options, this.host.textInput(options.multiline, options.border) as never);
  }

  createViewSlot(initialView: import("./values/view.ts").View): ViewSlot {
    const lowered = materializeView(initialView);
    if (lowered === undefined) throw tuiError("runtime", "native View materialization is unavailable");
    return new ViewSlot(this.host.createViewSlot(lowered as object));
  }

  createScrollPane(initialView: import("./values/view.ts").View): ScrollPane {
    const lowered = materializeView(initialView);
    if (lowered === undefined) throw tuiError("runtime", "native View materialization is unavailable");
    return new NativeScrollPane(this.host.scrollPane(lowered as object));
  }

  bindKey(key: string, routeId: string, modifiers?: readonly string[]): void {
    this.host.bindKey(key, modifiers, routeId);
  }

  route(output: OutputHandle<string>, routeId: string): void {
    this.host.route((output as unknown as { nativeObject: object }).nativeObject as never, routeId);
  }

  interceptPaste(input: TextInput, routeId: string): void {
    this.host.interceptPaste((input as unknown as { nativeHandle: object }).nativeHandle, routeId);
  }

  forwardPaste(text: string): void { this.host.forwardPaste(text); }

  async resize(width: number, height: number): Promise<void> {
    if (this.closed) throw tuiError("terminal", "TUI runtime is closed");
    validateSize(width, height);
    this.host.resize(width, height);
  }

  async close(): Promise<void> {
    if (this.closed) return;
    this.closed = true;
    this.host.dispose();
  }

  async exit(): Promise<void> {
    if (this.closed) return;
    this.host.exit();
    this.closed = true;
  }

  async setTheme(theme: import("./values/theme.ts").Theme): Promise<void> {
    if (this.closed) throw tuiError("terminal", "TUI runtime is closed");
    this.host.setTheme(theme.materialize());
  }

  enqueue(event: { readonly type: "key"; readonly key: string; readonly modifiers?: readonly string[] } | { readonly type: "paste"; readonly text: string } | { readonly type: "resize"; readonly width: number; readonly height: number }): void {
    if (event.type === "key") this.host.dispatchKey(event.key, event.modifiers);
    if (event.type === "paste") this.host.dispatchPaste(event.text);
    if (event.type === "resize") void this.resize(event.width, event.height);
  }

  screenRows(): readonly string[] { return this.host.screenRows(); }
  nativeHistoryRows(): readonly string[] { return this.host.nativeHistoryRows(); }
  styleAt(row: number, column: number): Readonly<Record<string, unknown>> {
    const style = this.host.styleAt(row, column) as Readonly<Record<string, unknown>> | null;
    if (style === null) throw tuiError("runtime", "native cell style is unavailable");
    return style;
  }
  cellXOfText(row: number, text: string): number | null { return this.host.cellXOfText(row, text); }
  exited(): boolean { return this.host.exited(); }
  advance(ms: number): void { this.host.advanceTime(ms); }
  current(): Scene | undefined { return this.currentScene; }
}

function ensureSignal(signal: AbortSignal | undefined): void {
  if (signal?.aborted) throw tuiError("cancelled", "TUI render was cancelled");
}

function validateSize(width: number, height: number): void {
  if (!Number.isInteger(width) || !Number.isInteger(height) || width <= 0 || height <= 0) throw asTuiError(new RangeError("terminal size must be positive integers"));
}

import { native } from "../native.ts";
import { materializeView } from "./materialize.ts";
import { asTuiError, tuiError } from "./errors.ts";
import { requireNativeClass } from "./handles.ts";
import { Scene } from "./scene.ts";
import { History } from "./history.ts";
import { TextInput } from "./text-input.ts";
import { ViewSlot } from "./component.ts";
import { WorkingActivity } from "./working.ts";
import type {
  OutputHandle,
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
      return new Tui(new Host(width, height, options.headless ?? false), width, height);
    } catch (error) {
      throw asTuiError(error);
    }
  }

  get size(): Promise<TerminalMetadata> { return Promise.resolve({ width: this.width, height: this.height }); }

  async nextAction(signal?: AbortSignal): Promise<{ actionId: string; payload?: string } | null> {
    while (!this.closed) {
      if (signal?.aborted) throw tuiError("cancelled", "TUI action wait was cancelled");
      const action = this.host.nextAction();
      if (action !== null) return { actionId: action.action_id, ...(action.payload === null || action.payload === undefined ? {} : { payload: action.payload }) };
      this.host.pollTerminal();
      await new Promise<void>((resolve) => setTimeout(resolve, 4));
    }
    return null;
  }

  /** Compatibility surface for callers that only need termination. */
  async nextEvent(signal?: AbortSignal): Promise<TuiEvent> {
    const action = await this.nextAction(signal);
    return { type: "terminate", reason: action === null ? "closed" : "action" };
  }

  async render(scene: SceneContract, signal?: AbortSignal): Promise<void> {
    ensureSignal(signal);
    if (this.closed) throw tuiError("terminal", "TUI runtime is closed");
    const normalized = Scene.from(scene);
    const lowered = materializeView(normalized.body);
    if (lowered === undefined) throw tuiError("runtime", "native View materialization is unavailable");
    this.host.render(lowered as object);
    this.currentScene = normalized;
  }

  createHistory(): History { return new History(this.host.history() as never); }

  createTextInput(options: { multiline?: boolean } = {}): TextInput {
    return new TextInput(options, this.host.textInput(options.multiline) as never);
  }

  createWorking(): WorkingActivity {
    return new WorkingActivity(this.host.working() as never);
  }

  createViewSlot(initialView: import("./values/view.ts").View): ViewSlot {
    const lowered = materializeView(initialView);
    if (lowered === undefined) throw tuiError("runtime", "native View materialization is unavailable");
    return new ViewSlot(this.host.createViewSlot(lowered as object));
  }

  bindKey(key: string, actionId: string, modifiers?: readonly string[]): void {
    this.host.bindKey(key, modifiers, actionId);
  }

  route(output: OutputHandle<string>, actionId: string): void {
    this.host.route((output as unknown as { nativeObject: object }).nativeObject as never, actionId);
  }

  interceptPaste(input: TextInput, actionId: string): void {
    this.host.interceptPaste((input as unknown as { nativeHandle: object }).nativeHandle, actionId);
  }

  forwardPaste(text: string): void { this.host.dispatchPaste(text); }

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

  enqueue(event: TuiEvent): void {
    if (event.type === "key") this.host.dispatchKey(event.key, event.modifiers);
    if (event.type === "paste") this.host.dispatchPaste(event.text);
    if (event.type === "resize") void this.resize(event.width, event.height);
  }

  screenRows(): readonly string[] { return this.host.screenRows(); }
  nativeHistoryRows(): readonly string[] { return this.host.nativeHistoryRows(); }
  advance(ms: number): void { this.host.advanceTime(ms); }
  current(): Scene | undefined { return this.currentScene; }
}

function ensureSignal(signal: AbortSignal | undefined): void {
  if (signal?.aborted) throw tuiError("cancelled", "TUI render was cancelled");
}

function validateSize(width: number, height: number): void {
  if (!Number.isInteger(width) || !Number.isInteger(height) || width <= 0 || height <= 0) throw asTuiError(new RangeError("terminal size must be positive integers"));
}

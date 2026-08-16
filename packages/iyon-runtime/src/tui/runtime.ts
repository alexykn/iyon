import { materializeView } from "./materialize.ts";
import { asTuiError, tuiError } from "./errors.ts";
import { resizeEvent, terminateEvent } from "./events.ts";
import { Scene } from "./scene.ts";
import type { Scene as SceneContract, TerminalMetadata, TuiEvent, TuiOpenOptions, TuiRuntime } from "./types.ts";

interface Waiter { resolve: (event: TuiEvent) => void; reject: (error: unknown) => void; signal?: AbortSignal; onAbort?: () => void; }

export class Tui implements TuiRuntime {
  private readonly events: TuiEvent[] = [];
  private readonly waiters: Waiter[] = [];
  private closed = false;
  private width: number;
  private height: number;
  private currentScene?: Scene;

  private constructor(options: TuiOpenOptions = {}) {
    this.width = options.width ?? 80;
    this.height = options.height ?? 24;
    validateSize(this.width, this.height);
    if (options.signal?.aborted) this.closed = true;
  }

  static async open(options: TuiOpenOptions = {}): Promise<Tui> { return new Tui(options); }

  get size(): Promise<TerminalMetadata> { return Promise.resolve({ width: this.width, height: this.height }); }

  nextEvent(signal?: AbortSignal): Promise<TuiEvent> {
    if (signal?.aborted) return Promise.reject(tuiError("cancelled", "TUI event wait was cancelled"));
    const event = this.events.shift();
    if (event !== undefined) return Promise.resolve(event);
    if (this.closed) return Promise.resolve(terminateEvent("closed"));
    return new Promise<TuiEvent>((resolve, reject) => {
      const waiter: Waiter = { resolve, reject, signal };
      if (signal !== undefined) {
        waiter.onAbort = () => {
          const index = this.waiters.indexOf(waiter);
          if (index >= 0) this.waiters.splice(index, 1);
          reject(tuiError("cancelled", "TUI event wait was cancelled"));
        };
        signal.addEventListener("abort", waiter.onAbort, { once: true });
      }
      this.waiters.push(waiter);
    });
  }

  async render(scene: SceneContract, signal?: AbortSignal): Promise<void> {
    ensureSignal(signal);
    if (this.closed) throw tuiError("terminal", "TUI runtime is closed");
    const normalized = Scene.from(scene);
    materializeView(normalized.body);
    this.currentScene = normalized;
  }

  async resize(width: number, height: number): Promise<void> {
    if (this.closed) throw tuiError("terminal", "TUI runtime is closed");
    validateSize(width, height);
    this.width = width;
    this.height = height;
    this.enqueue(resizeEvent(width, height));
  }

  async close(): Promise<void> {
    if (this.closed) return;
    this.closed = true;
    const event = terminateEvent("closed");
    for (const waiter of this.waiters.splice(0)) finishWaiter(waiter, event);
  }

  /** Internal input seam used by the headless harness and native event pump. */
  enqueue(event: TuiEvent): void {
    const waiter = this.waiters.shift();
    if (waiter === undefined) { this.events.push(event); return; }
    finishWaiter(waiter, event);
  }

  current(): Scene | undefined { return this.currentScene; }
}

function finishWaiter(waiter: Waiter, event: TuiEvent): void {
  if (waiter.signal !== undefined && waiter.onAbort !== undefined) waiter.signal.removeEventListener("abort", waiter.onAbort);
  waiter.resolve(event);
}

function ensureSignal(signal: AbortSignal | undefined): void {
  if (signal?.aborted) throw tuiError("cancelled", "TUI render was cancelled");
}

function validateSize(width: number, height: number): void {
  if (!Number.isInteger(width) || !Number.isInteger(height) || width <= 0 || height <= 0) throw asTuiError(new RangeError("terminal size must be positive integers"));
}

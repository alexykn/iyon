import { tuiError } from "./errors.ts";
import { pasteEvent, keyEvent } from "./events.ts";
import { Tui } from "./runtime.ts";
import { Scene } from "./scene.ts";
import type { AppHarness as AppHarnessContract, TuiEvent, TuiOpenOptions } from "./types.ts";
import { textRowsForHarness, type View } from "./values/view.ts";

export class AppHarness implements AppHarnessContract {
  private readonly tui: Tui;
  private readonly options: { width: number; height: number };
  private rows: string[] = [];
  private historyRows: string[] = [];
  private clock = 0;
  private didExit = false;

  private constructor(tui: Tui, options: { width: number; height: number }) { this.tui = tui; this.options = options; }

  static async open(options: TuiOpenOptions = {}): Promise<AppHarness> {
    const size = { width: options.width ?? 80, height: options.height ?? 24 };
    const tui = await Tui.open({ ...options, ...size, headless: true });
    return new AppHarness(tui, size);
  }

  get size(): Promise<{ width: number; height: number }> { return Promise.resolve(this.options); }
  nextEvent(signal?: AbortSignal): Promise<TuiEvent> { return this.tui.nextEvent(signal); }
  async render(scene: { readonly body: View; readonly history?: unknown }, signal?: AbortSignal): Promise<void> {
    await this.tui.render(Scene.from(scene as never), signal);
    this.rows = textRowsForHarness(scene.body);
    if (this.rows.length > this.options.height) {
      const split = this.rows.length - this.options.height;
      this.historyRows.push(...this.rows.slice(0, split));
      this.rows = this.rows.slice(split);
    }
  }
  resize(width: number, height: number): Promise<void> { this.options.width = width; this.options.height = height; return this.tui.resize(width, height); }
  async close(): Promise<void> { this.didExit = true; await this.tui.close(); }
  pressKey(key: string, modifiers?: readonly string[]): void { this.tui.enqueue(keyEvent(key, modifiers)); }
  paste(text: string): void { this.tui.enqueue(pasteEvent(text)); }
  advance(ms: number): void { if (!Number.isFinite(ms) || ms < 0) throw tuiError("validation", "clock advancement must be non-negative"); this.clock += ms; }
  screenRows(): readonly string[] { return [...this.rows]; }
  nativeHistoryRows(): readonly string[] { return [...this.historyRows]; }
  styleAt(_row: number, _column: number): Readonly<Record<string, unknown>> { return {}; }
  cellXOfText(row: number, text: string): number | null { const value = this.rows[row]; if (value === undefined) return null; const index = value.indexOf(text); return index < 0 ? null : index; }
  exited(): boolean { return this.didExit; }
  now(): number { return this.clock; }
}

export const createAppHarness = AppHarness.open;

import type { App } from "iyon:plugins";
import { History, Scene, TextInput, Tui } from "iyon:tui";
import { History as RuntimeHistory, TextInput as RuntimeTextInput, TextStream } from "@iyon/runtime/tui";
import type { History as HistoryHandle, TextInput as TextInputHandle, TuiRuntime, WorkingActivityHandle } from "@iyon/runtime/tui";
import type {
  IyonAgent,
  IyonCoreCommands,
  IyonModelMetadata,
  IyonState,
} from "./contracts.ts";
import { ComposerPasteStore } from "./composer.ts";
import { createInitialState, reduceIyonState } from "./state.ts";
import { createIyonTheme, type IyonTheme } from "./theme.ts";
import { createIyonView, userBatchView } from "./view.ts";
import { handleIyonAction } from "./actions.ts";
import { startCoreEventBridge, type CoreEventBridge, type CoreEventSource } from "./backend.ts";

export interface IyonAppDependencies {
  readonly agent: IyonAgent;
  readonly core: IyonCoreCommands;
  readonly model: IyonModelMetadata;
  readonly tui?: TuiRuntime;
}

export interface IyonApp extends App {
  readonly id: "iyon";
  readonly agent: IyonAgent;
  readonly core: IyonCoreCommands;
  readonly model: IyonModelMetadata;
  readonly history: HistoryHandle;
  readonly composer: TextInputHandle;
  readonly working?: WorkingActivityHandle;
  readonly theme: IyonTheme;
  readonly state: IyonState;
  start(tui?: TuiRuntime): Promise<void>;
  stop(): Promise<void>;
  run(signal?: AbortSignal): Promise<void>;
  handleAction(action: import("./contracts.ts").IyonAction): Promise<void>;
  startBackendBridge(source: CoreEventSource): CoreEventBridge;
}

export function createIyonApp(dependencies: IyonAppDependencies): IyonApp {
  return new IyonAppImpl(dependencies);
}

class IyonAppImpl implements IyonApp {
  [key: string]: unknown;
  readonly id = "iyon" as const;
  private historyHandle: HistoryHandle = new History();
  private composerHandle: TextInputHandle = new TextInput({ multiline: true });
  readonly pasteStore = new ComposerPasteStore();
  readonly theme: IyonTheme = createIyonTheme();
  private currentState: IyonState;
  private tui?: TuiRuntime;
  private ownsTui = false;
  private started = false;
  private exitAfterRender = false;
  private workingHandle?: WorkingActivityHandle;
  private assistantStream?: TextStream;
  private assistantText = "";
  private readonly toolStreams = new Map<string, TextStream>();
  private readonly toolNames = new Map<string, string>();
  private readonly renderedUserMessages = new Map<string, number>();
  private readonly pendingSteeringMessages = new Map<string, number>();

  constructor(
    readonly dependencies: IyonAppDependencies,
  ) {
    this.currentState = createInitialState(dependencies.model);
  }

  get state(): IyonState { return this.currentState; }
  get history(): HistoryHandle { return this.historyHandle; }
  get composer(): TextInputHandle { return this.composerHandle; }
  get working(): WorkingActivityHandle | undefined { return this.workingHandle; }
  get agent(): IyonAgent { return this.dependencies.agent; }
  get core(): IyonCoreCommands { return this.dependencies.core; }
  get model(): IyonModelMetadata { return this.dependencies.model; }

  async start(tui = this.dependencies.tui): Promise<void> {
    if (this.started) return;
    this.tui = tui ?? await Tui.open();
    this.ownsTui = tui === undefined;
    if (this.tui.createHistory !== undefined && this.tui.createTextInput !== undefined) {
      await this.historyHandle.dispose();
      await this.composerHandle.dispose();
      this.historyHandle = this.tui.createHistory() as unknown as RuntimeHistory;
      this.composerHandle = this.tui.createTextInput({ multiline: true }) as unknown as RuntimeTextInput;
      this.workingHandle = this.tui.createWorking?.();
      this.tui.bindKey?.("c", "ctrlC", ["control"]);
      this.tui.bindKey?.("Escape", "escape");
      this.tui.bindKey?.("Tab", "cycleReasoningEffort", ["shift"]);
      this.tui.route?.(await this.composerHandle.submitted(), "submit");
      this.tui.interceptPaste?.(this.composerHandle, "composerPaste");
    }
    this.started = true;
    await this.renderCurrentScene();
  }

  async stop(): Promise<void> {
    if (!this.started && this.tui === undefined) return;
    try {
      if (this.ownsTui) await this.tui?.close();
    } finally {
      await this.composerHandle.dispose();
      await this.historyHandle.dispose();
      await this.workingHandle?.dispose();
      await this.assistantStream?.dispose();
      for (const stream of this.toolStreams.values()) await stream.dispose();
      this.toolStreams.clear();
      this.toolNames.clear();
      this.renderedUserMessages.clear();
      this.pendingSteeringMessages.clear();
      this.workingHandle = undefined;
      this.assistantStream = undefined;
      this.tui = undefined;
      this.started = false;
      this.ownsTui = false;
    }
  }

  dispatch(action: import("./contracts.ts").IyonAction): void {
    const previous = this.currentState;
    this.currentState = reduceIyonState(this.currentState, action);
    if (this.started) {
      void this.appendHistory(action, previous, this.currentState).then(async (viewChanged) => {
        if (viewChanged) await this.renderCurrentScene();
      });
    }
  }

  async handleAction(action: import("./contracts.ts").IyonAction): Promise<void> {
    const result = await handleIyonAction(this.currentState, action, {
      core: this.core,
      agent: this.agent,
      pasteStore: this.pasteStore,
      clearComposer: () => this.composer.clear(),
      composerText: () => this.composer.text(),
      forwardPaste: (text) => this.tui?.forwardPaste?.(text),
      runAgent: () => this.agent.run?.(),
      onExit: () => { this.exitAfterRender = true; },
    });
    const previous = this.currentState;
    this.currentState = result.state;
    if (action.type === "submit" && previous.activeTurn) {
      this.incrementPendingSteering(action.text);
    }
    const viewChanged = await this.appendHistory(action, previous, this.currentState);
    if (viewChanged) await this.renderCurrentScene();
    if (result.exited && this.exitAfterRender) {
      this.exitAfterRender = false;
      await this.tui?.exit?.();
    }
  }

  private async appendHistory(
    action: import("./contracts.ts").IyonAction,
    previous: IyonState,
    _next: IyonState,
  ): Promise<boolean> {
    if (action.type === "submit" && action.text.length > 0 && !previous.activeTurn) {
      this.renderedUserMessages.set(action.text, (this.renderedUserMessages.get(action.text) ?? 0) + 1);
      await this.history.push(userBatchView([action.text], this.theme));
      return true;
    }
    if (action.type === "ctrlC" && _next.goodbye) {
      await this.history.push(View.text("Goodbye.").fillWidth());
      return true;
    }
    if (action.type !== "backend") return action.type === "submit";
    const event = action.event;
    if (event.type === "assistantDelta") {
      if (this.assistantStream === undefined) {
        this.assistantStream = new TextStream();
        this.assistantText = "";
        await this.history.pushStream(this.assistantStream);
      }
      this.assistantText += event.text;
      await this.assistantStream.update(this.assistantText);
      return false;
    }
    if (event.type === "userMessage") {
      if (this.consumePendingSteering(event.text)) return false;
      const rendered = this.renderedUserMessages.get(event.text) ?? 0;
      if (rendered > 0) {
        if (rendered === 1) this.renderedUserMessages.delete(event.text);
        else this.renderedUserMessages.set(event.text, rendered - 1);
      } else {
        await this.history.push(userBatchView([event.text], this.theme));
      }
      return false;
    }
    if (event.type === "turnFinished" || event.type === "turnFailed" || event.type === "turnCancelled") {
      if (this.assistantStream !== undefined) {
        await this.assistantStream.seal();
        this.assistantStream = undefined;
      }
      return true;
    }
    if (event.type === "toolCallStarted") {
      const stream = new TextStream();
      this.toolStreams.set(event.toolCallId, stream);
      this.toolNames.set(event.toolCallId, event.toolName);
      await this.history.pushStream(stream);
      await stream.update(`• ${event.toolName} - running`);
      return false;
    }
    if (event.type === "toolResult") {
      const stream = this.toolStreams.get(event.toolCallId) ?? new TextStream();
      if (!this.toolStreams.has(event.toolCallId)) {
        this.toolStreams.set(event.toolCallId, stream);
        await this.history.pushStream(stream);
      }
      const lines = event.text.split("\n");
      const collapsed = lines.length > 1 ? ` … ${lines.length - 1} more lines (full result retained)` : "";
      await stream.update(`• ${event.toolName} - ${event.isError ? "failed" : "finished"}${collapsed}`);
      await stream.seal();
      this.toolStreams.delete(event.toolCallId);
      this.toolNames.delete(event.toolCallId);
      return false;
    }
    if (event.type === "toolCallFinished") {
      const stream = this.toolStreams.get(event.toolCallId);
      if (stream !== undefined) {
        await stream.update(`• ${this.toolNames.get(event.toolCallId) ?? "tool"} - ${event.isError ? "failed" : "finished"}`);
        await stream.seal();
        this.toolStreams.delete(event.toolCallId);
        this.toolNames.delete(event.toolCallId);
      }
      return false;
    }
    return event.type === "turnStarted" || event.type === "steerQueued" || event.type === "configChanged"
      || event.type === "toolApprovalRequested" || event.type === "toolApprovalResolved";
  }

  private incrementPendingSteering(text: string): void {
    this.pendingSteeringMessages.set(text, (this.pendingSteeringMessages.get(text) ?? 0) + 1);
  }

  private consumePendingSteering(text: string): boolean {
    const count = this.pendingSteeringMessages.get(text) ?? 0;
    if (count === 0) return false;
    if (count === 1) this.pendingSteeringMessages.delete(text);
    else this.pendingSteeringMessages.set(text, count - 1);
    return true;
  }

  private async renderCurrentScene(): Promise<void> {
    if (this.tui !== undefined && this.started) {
      await this.workingHandle?.setActive(this.currentState.working);
      await this.workingHandle?.setPending(this.currentState.steering);
      await this.tui.render(new Scene(createIyonView({ composer: this.composer, history: this.history, state: this.currentState, theme: this.theme, working: this.workingHandle }), this.history));
    }
  }

  async run(signal?: AbortSignal): Promise<void> {
    await this.start();
    const tui = this.tui;
    if (tui?.nextAction === undefined) return;
    while (!signal?.aborted && !this.state.goodbye) {
      const action = await tui.nextAction(signal);
      if (action === null) return;
      await this.handleAction(actionFromNative(action));
      if (this.state.goodbye) return;
    }
  }

  startBackendBridge(source: CoreEventSource): CoreEventBridge { return startCoreEventBridge(source, this); }
}

function actionFromNative(action: { readonly actionId: string; readonly payload?: string }): import("./contracts.ts").IyonAction {
  switch (action.actionId) {
    case "submit": return { type: "submit", text: action.payload ?? "" };
    case "composerPaste": return { type: "composerPaste", text: action.payload ?? "" };
    case "ctrlC": return { type: "ctrlC" };
    case "escape": return { type: "escape" };
    case "cycleReasoningEffort": return { type: "cycleReasoningEffort" };
    default: throw new Error(`unknown Iyon TUI action: ${action.actionId}`);
  }
}

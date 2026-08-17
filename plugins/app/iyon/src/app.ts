import type { App } from "iyon:plugins";
import { History, Scene, Style, TextInput, Tui, View } from "iyon:tui";
import { renderGenericCall, renderGenericResult } from "@iyon/runtime";
import { History as RuntimeHistory, TextInput as RuntimeTextInput } from "@iyon/runtime/tui";
import type { History as HistoryHandle, TextInput as TextInputHandle, TuiRuntime, ViewSlot, WorkingActivityHandle } from "@iyon/runtime/tui";
import type { ToolCall, ToolResult } from "@iyon/sdk";
import type {
  IyonAgent,
  IyonCoreCommands,
  IyonModelMetadata,
  IyonState,
  LiveTool,
  ToolResolver,
} from "./contracts.ts";
import { ComposerPasteStore } from "./composer.ts";
import { ApprovalStore } from "./approvals.ts";
import { createInitialState, hasActiveWork, reduceIyonState } from "./state.ts";
import { createIyonTheme, type IyonTheme } from "./theme.ts";
import { createIyonView, userBatchView } from "./view.ts";
import { handleIyonAction } from "./actions.ts";
import { startCoreEventBridge, type CoreEventBridge, type CoreEventSource } from "./backend.ts";
import { NativeAssistantStream } from "./streaming.ts";
import { ToolCardStore } from "./tool-cards.ts";

export interface IyonAppDependencies {
  readonly agent: IyonAgent;
  readonly core: IyonCoreCommands;
  readonly model: IyonModelMetadata;
  readonly tools?: ToolResolver;
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
  private exiting = false;
  private workingHandle?: WorkingActivityHandle;
  private assistantStream?: NativeAssistantStream;
  private readonly toolCards = new ToolCardStore();
  private readonly toolSlots = new Map<string, ViewSlot>();
  private readonly mountedToolCards = new Set<string>();
  private readonly renderedToolResults = new Set<string>();
  private readonly approvals = new ApprovalStore();
  private activeAgentRun?: Promise<void>;

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
    await this.tui.setTheme?.(this.theme);
    if (this.tui.createHistory !== undefined && this.tui.createTextInput !== undefined) {
      await this.historyHandle.dispose();
      await this.composerHandle.dispose();
      this.historyHandle = this.tui.createHistory() as unknown as RuntimeHistory;
      this.composerHandle = this.tui.createTextInput({
        multiline: true,
        border: { style: "plain", edges: "topBottom", color: this.theme.inputBorder },
      }) as unknown as RuntimeTextInput;
      if (this.tui.createWorking === undefined) throw new Error("native working activity is unavailable");
      this.workingHandle = this.tui.createWorking();
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
      for (const slot of this.toolSlots.values()) await slot.dispose();
      this.toolSlots.clear();
      this.mountedToolCards.clear();
      this.toolCards.clear();
      this.renderedToolResults.clear();
      this.approvals.clear();
      this.workingHandle = undefined;
      this.assistantStream = undefined;
      this.activeAgentRun = undefined;
      this.tui = undefined;
      this.started = false;
      this.ownsTui = false;
      this.exiting = false;
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
    if (this.exiting) return;
    const result = await handleIyonAction(this.currentState, action, {
      core: this.core,
      agent: this.agent,
      pasteStore: this.pasteStore,
      clearComposer: () => this.composer.clear(),
      composerText: () => this.composer.text(),
      forwardPaste: (text) => this.tui?.forwardPaste?.(text),
      runAgent: () => this.startAgentRun(),
      onExit: () => { this.exitAfterRender = true; },
    });
    const previous = this.currentState;
    this.currentState = result.state;
    const viewChanged = await this.appendHistory(action, previous, this.currentState);
    if (viewChanged) await this.renderCurrentScene();
    if ((result.exited && this.exitAfterRender) || (action.type === "requestExit" && result.state.goodbye)) {
      this.exitAfterRender = false;
      await this.shutdown();
    }
  }

  private startAgentRun(): Promise<void> {
    if (this.activeAgentRun !== undefined) return Promise.resolve();
    const run = this.agent.run?.();
    if (run === undefined) return Promise.resolve();
    this.activeAgentRun = Promise.resolve(run).then(() => undefined)
      .catch((error: unknown) => {
        this.dispatch({ type: "backend", event: { type: "turnFailed", message: error instanceof Error ? error.message : String(error) } });
      })
      .finally(() => { this.activeAgentRun = undefined; });
    return Promise.resolve();
  }

  private async appendHistory(
    action: import("./contracts.ts").IyonAction,
    previous: IyonState,
    next: IyonState,
  ): Promise<boolean> {
    if (action.type === "submit" && action.text.length > 0 && !hasActiveWork(previous)) {
      await this.history.push(userBatchView([action.text], this.theme));
      return true;
    }
    if (action.type !== "backend") {
      return action.type === "submit" || action.type === "cycleReasoningEffort";
    }
    const event = action.event;
    if (event.type === "assistantDelta") {
      await this.openAssistantStream();
      await this.assistantStream?.append("text", event.text);
      return true;
    }
    if (event.type === "thinkingDelta") {
      await this.openAssistantStream();
      await this.assistantStream?.append("thinking", event.text);
      return true;
    }
    if (event.type === "userMessage") {
      await this.history.push(userBatchView([event.text], this.theme));
      return false;
    }
    if (event.type === "turnStarted" || event.type === "steerQueued") {
      return true;
    }
    if (event.type === "turnFinished" || event.type === "turnFailed" || event.type === "turnCancelled") {
      await this.sealAssistantStream();
      for (const [key, card] of next.liveTools) {
        if (card.frozen && !previous.liveTools.get(key)?.frozen) {
          if (event.type === "turnCancelled" && card.toolCallId !== undefined) this.toolCards.cancel(String(card.toolCallId));
          await this.updateToolSlot(key, card);
        }
      }
      return true;
    }
    if (event.type === "toolCallPreparing") {
      await this.sealAssistantStream();
      this.toolCards.preparing(event.key, event.toolCallId, event.toolName);
      await this.updateToolSlot(this.toolCards.keyForDraft(event.key), next.liveTools.get(this.toolCards.keyForDraft(event.key)));
      return false;
    }
    if (event.type === "toolCallArguments") {
      this.toolCards.arguments(event.key, event.delta, event.toolCallId, event.toolName);
      const key = this.toolCards.keyForDraft(event.key);
      await this.updateToolSlot(key, next.liveTools.get(key));
      return false;
    }
    if (event.type === "toolCallPrepared") {
      this.toolCards.prepared(event.key, event.toolCallId, event.toolName, event.arguments);
      const key = this.toolCards.keyForDraft(event.key);
      await this.updateToolSlot(key, next.liveTools.get(key));
      return false;
    }
    if (event.type === "toolCallStarted") {
      await this.sealAssistantStream();
      const card = this.toolCards.started(event.toolCallId, event.toolName, event.arguments);
      const key = this.toolCards.keyFor(event.toolCallId) ?? event.toolCallId;
      await this.updateToolSlot(key, next.liveTools.get(key) ?? card);
      return false;
    }
    if (event.type === "toolCallUpdated") {
      this.toolCards.update(event.toolCallId, event.update);
      const key = this.toolCards.keyFor(event.toolCallId);
      if (key !== undefined) await this.updateToolSlot(key, next.liveTools.get(key));
      return false;
    }
    if (event.type === "toolApprovalRequested") {
      this.approvals.request({ approvalId: event.approvalId, toolCallId: event.toolCallId, toolName: event.toolName, arguments: event.arguments });
      this.toolCards.approval(event.toolCallId);
      const key = this.toolCards.keyFor(event.toolCallId);
      if (key !== undefined) await this.updateToolSlot(key, next.liveTools.get(key));
      return true;
    }
    if (event.type === "toolApprovalResolved") {
      this.approvals.resolve(event.approvalId);
      this.toolCards.resolveApproval(event.toolCallId, event.approved);
      const key = this.toolCards.keyFor(event.toolCallId);
      if (key !== undefined) await this.updateToolSlot(key, next.liveTools.get(key));
      return true;
    }
    if (event.type === "toolResult") {
      const card = this.toolCards.result(event.toolCallId, event.toolName, event.text, event.details, event.isError);
      const key = this.toolCards.keyFor(event.toolCallId);
      if (key !== undefined) {
        this.renderedToolResults.add(key);
        await this.updateToolSlotResult(key, {
          content: [{ type: "text", text: event.text }],
          details: event.details,
          isError: event.isError,
          toolCallId: event.toolCallId as never,
          toolName: event.toolName,
          text: event.text,
        });
      } else if (card !== undefined) {
        await this.updateToolSlot(event.toolCallId, card);
      }
      return false;
    }
    if (event.type === "toolCallFinished") {
      const key = this.toolCards.keyFor(event.toolCallId);
      if (key !== undefined && !this.renderedToolResults.has(key)) {
        this.toolCards.finish(event.toolCallId, event.isError);
        await this.updateToolSlot(key, next.liveTools.get(key));
      }
      return false;
    }
    return false;
  }

  private async openAssistantStream(): Promise<void> {
    if (this.assistantStream !== undefined) return;
    this.assistantStream = new NativeAssistantStream();
    await this.history.pushStream(this.assistantStream.native);
  }

  private async sealAssistantStream(): Promise<void> {
    if (this.assistantStream === undefined) return;
    await this.assistantStream.seal();
    await this.history.sealStream(this.assistantStream.native);
    this.assistantStream = undefined;
  }

  private async updateToolSlot(key: string, card: LiveTool | undefined): Promise<void> {
    if (card === undefined) return;
    const view = this.renderToolCall(card, key);
    const slot = this.toolSlots.get(key);
    if (slot !== undefined) {
      await slot.setView(view as never);
      return;
    }
    if (this.tui?.createViewSlot === undefined) {
      if (this.mountedToolCards.has(key)) return;
      this.mountedToolCards.add(key);
      await this.history.push(view as never);
      return;
    }
    const created = this.tui.createViewSlot(view as never);
    this.toolSlots.set(key, created);
    this.mountedToolCards.add(key);
    await this.history.push(View.component(created).fillWidth());
  }

  private async updateToolSlotResult(key: string, result: ToolResult): Promise<void> {
    const slot = this.toolSlots.get(key);
    const view = this.renderToolResult(result);
    if (slot !== undefined) {
      await slot.setView(view as never);
      return;
    }
    if (!this.mountedToolCards.has(key)) {
      this.mountedToolCards.add(key);
      await this.history.push(view as never);
    }
  }

  private renderToolCall(card: LiveTool, key: string) {
    const call: ToolCall = {
      id: (card.toolCallId ?? key) as never,
      name: card.toolName ?? "tool",
      arguments: card.arguments ?? {},
      state: card.status,
      showArgPreview: card.arguments !== undefined,
    };
    const contribution = card.arguments === undefined ? undefined : this.dependencies.tools?.get(call.name);
    const callView = contribution?.renderCall?.(call) ?? renderGenericCall(call);
    const update = toolUpdateText(card);
    if (update === undefined) return callView;
    const output = View.hanging(
      View.text("  ").noWrap(),
      View.text("  ").noWrap(),
      View.text(update).style(Style.new().theme("text.muted")).fillWidth(),
    ).fillWidth();
    return View.vertical([callView, output]).fillWidth();
  }

  private renderToolResult(result: ToolResult) {
    const contribution = result.toolName === undefined ? undefined : this.dependencies.tools?.get(result.toolName);
    return contribution?.renderResult?.(result) ?? renderGenericResult(result);
  }

  private async shutdown(): Promise<void> {
    if (this.exiting) return;
    this.exiting = true;
    await this.sealAssistantStream();
    await this.workingHandle?.setActive(false);
    await this.workingHandle?.setPending([]);
    const pendingApproval = this.currentState.pendingApproval;
    if (pendingApproval !== undefined) {
      await this.core.reject?.(pendingApproval.approvalId, "application exiting");
      this.approvals.resolve(pendingApproval.approvalId);
    }
    const liveTools = new Map(this.currentState.liveTools);
    for (const [key, card] of liveTools) {
      if (card.frozen) continue;
      const cancelled = { ...card, status: "cancelled" as const, frozen: true, isError: true };
      liveTools.set(key, cancelled);
      if (card.toolCallId !== undefined) this.toolCards.finish(String(card.toolCallId), true);
      await this.updateToolSlot(key, cancelled);
    }
    this.currentState = { ...this.currentState, activeTurn: false, assistantOpen: false, goodbye: true, liveTools, pendingApproval: undefined, working: false, activityVisible: false };
    await this.history.push(View.text("Goodbye.").fillWidth());
    await this.renderCurrentScene();
    await this.tui?.exit?.();
  }

  private async renderCurrentScene(): Promise<void> {
    if (this.tui !== undefined && this.started) {
      await this.workingHandle?.setActive(this.currentState.activityVisible);
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

function toolUpdateText(card: LiveTool): string | undefined {
  if (card.text.length > 0) return card.text;
  if (card.progress !== undefined) {
    const { label, current, total } = card.progress;
    if (current !== undefined && total !== undefined) return `${label}: ${current}/${total}`;
    if (current !== undefined) return `${label}: ${current}`;
    if (total !== undefined) return `${label}: 0/${total}`;
    return label;
  }
  return card.details === undefined ? undefined : JSON.stringify(card.details);
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

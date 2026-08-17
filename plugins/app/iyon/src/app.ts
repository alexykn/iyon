import type { App } from "iyon:plugins";
import { History, Scene, Style, TextInput, Tui, View } from "iyon:tui";
import { renderGenericCall, renderGenericResult } from "@iyon/runtime";
import { collapseResultView } from "@iyon/plugins";
import { History as RuntimeHistory, TextInput as RuntimeTextInput } from "@iyon/runtime/tui";
import type { History as HistoryHandle, ScrollPane, TextInput as TextInputHandle, TuiRuntime, ViewSlot, WorkingActivityHandle } from "@iyon/runtime/tui";
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
import { createInitialState, reduceIyonState } from "./state.ts";
import { createIyonTheme, type IyonTheme } from "./theme.ts";
import { createIyonView, userBatchView } from "./view.ts";
import { handleIyonAction } from "./actions.ts";
import { startCoreEventBridge, type CoreEventBridge, type CoreEventSource } from "./backend.ts";
import { NativeAssistantStream } from "./streaming.ts";
import { ToolCardStore } from "./tool-cards.ts";

interface LiveUserBatch {
  readonly unit: number;
  readonly slot: ViewSlot;
  readonly messages: string[];
  readonly queueId?: number;
}

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
  flush(): Promise<void>;
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
  private readonly toolPanes = new Map<string, ScrollPane>();
  private readonly toolHistoryUnits = new Map<string, number>();
  private readonly mountedToolCards = new Set<string>();
  private readonly renderedToolResults = new Set<string>();
  private readonly approvals = new ApprovalStore();
  private activeAgentRun?: Promise<void>;
  private shutdownPromise?: Promise<void>;
  private shutdownComplete = false;
  private liveUserBatch?: LiveUserBatch;
  private historyMutation: Promise<void> = Promise.resolve();

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
  async flush(): Promise<void> { await this.historyMutation; }

  async start(tui = this.dependencies.tui): Promise<void> {
    if (this.started) return;
    this.tui = tui ?? await Tui.open();
    this.ownsTui = tui === undefined;
    this.shutdownComplete = false;
    this.shutdownPromise = undefined;
    if (this.tui.bindKey === undefined || this.tui.route === undefined) {
      throw new Error("native TUI action bindings are unavailable");
    }
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
      this.tui.bindKey("c", "ctrlC", ["control"]);
      this.tui.bindKey("\u0003", "ctrlC");
      this.tui.bindKey("Escape", "escape");
      this.tui.bindKey("Tab", "cycleReasoningEffort", ["shift"]);
      this.tui.route(await this.composerHandle.submitted(), "submit");
      this.tui.interceptPaste?.(this.composerHandle, "composerPaste");
    }
    this.started = true;
    await this.renderCurrentScene();
  }

  async stop(): Promise<void> {
    if (!this.started && this.tui === undefined) return;
    await this.historyMutation;
    try {
      if (this.ownsTui && !this.shutdownComplete) await this.tui?.close();
    } finally {
      await this.composerHandle.dispose();
      await this.historyHandle.dispose();
      await this.workingHandle?.dispose();
      await this.assistantStream?.dispose();
      for (const slot of this.toolSlots.values()) await slot.dispose();
      this.toolSlots.clear();
      for (const pane of this.toolPanes.values()) await pane.dispose();
      this.toolPanes.clear();
      this.toolHistoryUnits.clear();
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
      this.shutdownComplete = false;
      this.shutdownPromise = undefined;
      this.liveUserBatch = undefined;
      this.historyMutation = Promise.resolve();
    }
  }

  dispatch(action: import("./contracts.ts").IyonAction): Promise<void> {
    const previous = this.currentState;
    this.currentState = reduceIyonState(this.currentState, action);
    const next = this.currentState;
    if (this.started) {
      this.historyMutation = this.historyMutation.then(async () => {
        const viewChanged = await this.appendHistory(action, previous, next);
        if (viewChanged) await this.renderCurrentScene();
      });
    }
    return this.historyMutation;
  }

  async handleAction(action: import("./contracts.ts").IyonAction): Promise<void> {
    if (this.exiting) return;
    try {
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
      await this.historyMutation;
      const effectiveAction = action.type === "submit" && result.queueId !== undefined ? { ...action, queueId: result.queueId } : action;
      const viewChanged = await this.appendHistory(effectiveAction, previous, this.currentState);
      if (viewChanged) await this.renderCurrentScene();
      if ((result.exited && this.exitAfterRender) || (action.type === "requestExit" && result.state.goodbye)) {
        this.exitAfterRender = false;
        await this.shutdown();
      }
    } catch (error) {
      if (this.exiting) throw error;
      await this.recordTurnFailure(error);
    }
  }

  private startAgentRun(): Promise<void> {
    if (this.activeAgentRun !== undefined) return Promise.resolve();
    let run: Promise<unknown> | void;
    try {
      run = this.agent.run?.();
    } catch (error) {
      return Promise.reject(error);
    }
    if (run === undefined) return Promise.resolve();
    this.activeAgentRun = Promise.resolve(run).then(() => undefined)
      .catch((error: unknown) => {
        return this.recordTurnFailure(error);
      })
      .finally(() => { this.activeAgentRun = undefined; });
    return Promise.resolve();
  }

  private async recordTurnFailure(error: unknown): Promise<void> {
    const action = {
      type: "backend" as const,
      event: { type: "turnFailed" as const, message: error instanceof Error ? error.message : String(error) },
    };
    const previous = this.currentState;
    this.currentState = reduceIyonState(this.currentState, action);
    const viewChanged = await this.appendHistory(action, previous, this.currentState);
    if (viewChanged) await this.renderCurrentScene();
  }

  private async appendHistory(
    action: import("./contracts.ts").IyonAction,
    previous: IyonState,
    next: IyonState,
  ): Promise<boolean> {
    if (action.type !== "backend") {
      if (action.type === "submit") {
        await this.openUserBatch(action.text, action.queueId);
        return true;
      }
      return action.type === "cycleReasoningEffort";
    }
    const event = action.event;
    if (event.type === "assistantDelta") {
      await this.freezeUserBatch();
      await this.openAssistantStream();
      await this.assistantStream?.append("text", event.text);
      return true;
    }
    if (event.type === "thinkingDelta") {
      await this.freezeUserBatch();
      await this.openAssistantStream();
      await this.assistantStream?.append("thinking", event.text);
      return true;
    }
    if (event.type === "userMessage") {
      if (this.liveUserBatch !== undefined && event.queueId !== undefined && Number(event.queueId) === this.liveUserBatch.queueId) {
        if (this.liveUserBatch.messages.at(-1) !== event.text) {
          this.liveUserBatch.messages.push(event.text);
          await this.liveUserBatch.slot.setView(userBatchView(this.liveUserBatch.messages, this.theme) as never);
        }
        return true;
      }
      if (event.queueId === undefined) await this.history.push(userBatchView([event.text], this.theme));
      return event.queueId === undefined;
    }
    if (event.type === "turnStarted" || event.type === "steerQueued") {
      return true;
    }
    if (event.type === "turnFinished" || event.type === "turnFailed" || event.type === "turnCancelled") {
      await this.sealAssistantStream();
      for (const [key, card] of next.liveTools) {
        if (this.renderedToolResults.has(key)) continue;
        if (card.frozen && !previous.liveTools.get(key)?.frozen) {
          if (event.type === "turnCancelled" && card.toolCallId !== undefined) this.toolCards.cancel(String(card.toolCallId));
          await this.updateToolSlot(key, card);
        }
      }
      return true;
    }
    if (event.type === "toolCallPreparing") {
      await this.freezeUserBatch();
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
      await this.freezeUserBatch();
      await this.sealAssistantStream();
      const card = this.toolCards.started(event.toolCallId, event.toolName, event.arguments);
      const key = this.toolCards.keyFor(event.toolCallId) ?? event.toolCallId;
      await this.updateToolSlot(key, next.liveTools.get(key) ?? card);
      return false;
    }
    if (event.type === "toolCallUpdated") {
      this.toolCards.update(event.toolCallId, event.update);
      const key = this.toolCards.keyFor(event.toolCallId);
      if (key !== undefined) {
        if (this.toolPanes.has(key)) await this.updateToolContent(key, next.liveTools.get(key));
        else await this.updateToolSlot(key, next.liveTools.get(key));
      }
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
      return false;
    }
    return false;
  }

  private async openAssistantStream(): Promise<void> {
    if (this.assistantStream !== undefined) return;
    this.assistantStream = new NativeAssistantStream();
    await this.history.pushStream(this.assistantStream.native);
  }

  private async openUserBatch(text: string, queueId?: number): Promise<void> {
    if (this.liveUserBatch !== undefined) {
      this.liveUserBatch.messages.push(text);
      await this.liveUserBatch.slot.setView(userBatchView(this.liveUserBatch.messages, this.theme) as never);
      return;
    }
    if (this.tui?.createViewSlot === undefined) {
      await this.history.push(userBatchView([text], this.theme));
      return;
    }
    const slot = this.tui.createViewSlot(userBatchView([text], this.theme) as never);
    const unit = await this.history.push(View.component(slot).fillWidth());
    this.liveUserBatch = { unit, slot, messages: [text], queueId };
  }

  private async freezeUserBatch(): Promise<void> {
    const batch = this.liveUserBatch;
    if (batch === undefined) return;
    await this.history.freeze(batch.unit, userBatchView(batch.messages, this.theme));
    await batch.slot.dispose();
    this.liveUserBatch = undefined;
  }

  private async sealAssistantStream(): Promise<void> {
    if (this.assistantStream === undefined) return;
    await this.assistantStream.seal();
    await this.history.sealStream(this.assistantStream.native);
    this.assistantStream = undefined;
  }

  private async updateToolSlot(key: string, card: LiveTool | undefined): Promise<void> {
    if (card === undefined) return;
    const view = this.renderToolCall(card, key, false);
    if (card.frozen) {
      await this.freezeToolSlot(key, view);
      return;
    }
    const pulsing = !["finished", "failed", "cancelled"].includes(card.status);
    const slot = this.toolSlots.get(key);
    if (slot !== undefined) {
      await this.updateToolContent(key, card);
      if (pulsing) {
        await slot.setAnimation([view as never, this.renderToolCall(card, key, true) as never], 480);
      } else {
        await slot.stopAnimation(view as never);
      }
      return;
    }
    if (this.tui?.createViewSlot === undefined || this.tui.createScrollPane === undefined) {
      if (this.mountedToolCards.has(key)) return;
      this.mountedToolCards.add(key);
      await this.history.push(View.vertical([view, this.renderToolUpdate(card)]).fillWidth() as never);
      return;
    }
    const pane = this.tui.createScrollPane(View.spacer(0));
    this.toolPanes.set(key, pane);
    const created = this.tui.createViewSlot(view as never);
    this.toolSlots.set(key, created);
    this.mountedToolCards.add(key);
    await this.updateToolContent(key, card);
    const historyUnit = await this.history.push(View.vertical((column) => {
      column.child(View.component(created).fillWidth());
      column.flexMax(16, View.component(pane).fillWidth());
    }).fillWidth());
    this.toolHistoryUnits.set(key, historyUnit);
    if (pulsing) await created.setAnimation([view as never, this.renderToolCall(card, key, true) as never], 480);
  }

  private async updateToolContent(key: string, card: LiveTool | undefined): Promise<void> {
    if (card === undefined) return;
    const pane = this.toolPanes.get(key);
    if (pane === undefined) return;
    await pane.setContent(this.renderToolUpdate(card));
    await pane.followEnd();
  }

  private async updateToolSlotResult(key: string, result: ToolResult): Promise<void> {
    const slot = this.toolSlots.get(key);
    const card = this.toolCards.getByKey(key);
    const call = card === undefined ? undefined : this.renderToolCall(card, key, false);
    const resultView = this.renderToolResult(result);
    const view = call === undefined ? resultView : View.vertical([call, resultView]).fillWidth();
    if (slot !== undefined) {
      await this.freezeToolSlot(key, view);
      return;
    }
    if (!this.mountedToolCards.has(key)) {
      this.mountedToolCards.add(key);
      await this.history.push(view as never);
    }
  }

  private async freezeToolSlot(key: string, view: unknown): Promise<void> {
    const unit = this.toolHistoryUnits.get(key);
    if (unit === undefined) return;
    await this.history.freeze(unit, view as never);
    const slot = this.toolSlots.get(key);
    if (slot !== undefined) await slot.dispose();
    const pane = this.toolPanes.get(key);
    if (pane !== undefined) await pane.dispose();
    this.toolHistoryUnits.delete(key);
    this.toolSlots.delete(key);
    this.toolPanes.delete(key);
  }

  private renderToolCall(card: LiveTool, key: string, pulse: boolean) {
    const call: ToolCall = {
      id: (card.toolCallId ?? key) as never,
      name: card.toolName ?? "tool",
      arguments: card.arguments,
      state: card.status,
      argumentPreview: card.argumentPreview,
      showArgPreview: false,
      pulse,
    };
    const contribution = this.dependencies.tools?.get(call.name);
    const callView = contribution?.renderCall?.(call) ?? renderGenericCall(call);
    return callView;
  }

  private renderToolUpdate(card: LiveTool) {
    const update = toolUpdateText(card);
    if (update === undefined) return View.spacer(0);
    const output = View.hanging(
      View.text("  ").noWrap(),
      View.text("  ").noWrap(),
      View.text(update).style(Style.new().theme("text.muted")).fillWidth(),
    ).fillWidth();
    return output;
  }

  private renderToolResult(result: ToolResult) {
    const contribution = result.toolName === undefined ? undefined : this.dependencies.tools?.get(result.toolName);
    const view = contribution?.renderResult?.(result) ?? renderGenericResult(result);
    return collapseResultView(view);
  }

  private async shutdown(): Promise<void> {
    if (this.shutdownPromise !== undefined) return this.shutdownPromise;
    this.shutdownPromise = this.performShutdown();
    return this.shutdownPromise;
  }

  private async performShutdown(): Promise<void> {
    if (this.shutdownComplete) return;
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
    await this.tui?.exit();
    this.shutdownComplete = true;
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
    if (tui === undefined || tui.nextAction === undefined) throw new Error("native TUI action driver is unavailable");
    while (!this.state.goodbye) {
      if (signal?.aborted) {
        await this.shutdown();
        return;
      }
      let action;
      try {
        action = await tui.nextAction(signal);
      } catch (error) {
        if (signal?.aborted) {
          await this.shutdown();
          return;
        }
        await this.recordTurnFailure(error);
        await this.shutdown();
        return;
      }
      if (action === null) {
        await this.shutdown();
        return;
      }
      await this.handleAction(actionFromNative(action));
    }
  }

  startBackendBridge(source: CoreEventSource): CoreEventBridge { return startCoreEventBridge(source, this); }
}

function toolUpdateText(card: LiveTool): string | undefined {
  if (card.update?.type === "text" && card.update.text.length > 0) return card.update.text;
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

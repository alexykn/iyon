import type { App } from "iyon:plugins";
import { History, Scene, TextInput, Tui } from "iyon:tui";
import type { History as HistoryHandle, StyleSpec, TextInput as TextInputHandle, TuiRuntime, View as ViewValue } from "@iyon/runtime/tui";
import type {
  IyonAgent,
  IyonCoreCommands,
  IyonModelMetadata,
  IyonState,
} from "./contracts.ts";
import { ComposerPasteStore } from "./composer.ts";
import { createInitialState, reduceIyonState } from "./state.ts";
import { createIyonTheme, type IyonTheme } from "./theme.ts";
import { createIyonView } from "./view.ts";

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
  readonly theme: IyonTheme;
  readonly state: IyonState;
  start(tui?: TuiRuntime): Promise<void>;
  stop(): Promise<void>;
}

export function createIyonApp(dependencies: IyonAppDependencies): IyonApp {
  return new IyonAppImpl(dependencies);
}

class IyonAppImpl implements IyonApp {
  [key: string]: unknown;
  readonly id = "iyon" as const;
  readonly history = new History();
  readonly composer = new TextInput({ multiline: true });
  readonly pasteStore = new ComposerPasteStore();
  readonly theme: IyonTheme = createIyonTheme();
  private currentState: IyonState;
  private tui?: TuiRuntime;
  private ownsTui = false;
  private started = false;

  constructor(
    readonly dependencies: IyonAppDependencies,
  ) {
    this.currentState = createInitialState(dependencies.model);
  }

  get state(): IyonState { return this.currentState; }
  get agent(): IyonAgent { return this.dependencies.agent; }
  get core(): IyonCoreCommands { return this.dependencies.core; }
  get model(): IyonModelMetadata { return this.dependencies.model; }

  async start(tui = this.dependencies.tui): Promise<void> {
    if (this.started) return;
    this.tui = tui ?? await Tui.open({ headless: true });
    this.ownsTui = tui === undefined;
    await this.tui.render(new Scene(createIyonView({ composer: this.composer, history: this.history, state: this.currentState, theme: this.theme }), this.history));
    this.started = true;
  }

  async stop(): Promise<void> {
    if (!this.started && this.tui === undefined) return;
    try {
      if (this.ownsTui) await this.tui?.close();
    } finally {
      await this.composer.dispose();
      await this.history.dispose();
      this.tui = undefined;
      this.started = false;
      this.ownsTui = false;
    }
  }

  dispatch(action: import("./contracts.ts").IyonAction): void { this.currentState = reduceIyonState(this.currentState, action); }
}

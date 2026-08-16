import type { App } from "iyon:plugins";
import { History, Scene, Style, TextInput, Tui, View } from "iyon:tui";
import type { History as HistoryHandle, StyleSpec, TextInput as TextInputHandle, TuiRuntime, View as ViewValue } from "@iyon/runtime/tui";
import type {
  IyonAgent,
  IyonCoreCommands,
  IyonModelMetadata,
  IyonState,
} from "./contracts.ts";

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

const initialState = (model: IyonModelMetadata): IyonState => ({
  info: {
    status: "",
    provider: model.provider,
    modelId: model.modelId,
    reasoningEffort: model.reasoningEffort ?? "none",
  },
  composerText: "",
  userBatches: [],
  working: false,
  steering: [],
  assistantText: "",
  thinkingText: "",
  liveTools: new Map(),
  draftTools: new Map(),
  activeTurn: false,
  goodbye: false,
});

export function createIyonApp(dependencies: IyonAppDependencies): IyonApp {
  return new IyonAppImpl(dependencies);
}

export interface IyonTheme {
  readonly footer: StyleSpec;
}

class IyonAppImpl implements IyonApp {
  [key: string]: unknown;
  readonly id = "iyon" as const;
  readonly history = new History();
  readonly composer = new TextInput({ multiline: true });
  readonly theme: IyonTheme = { footer: Style.new().dim() };
  readonly state: IyonState;
  private tui?: TuiRuntime;
  private ownsTui = false;
  private started = false;

  constructor(
    readonly dependencies: IyonAppDependencies,
  ) {
    this.state = initialState(dependencies.model);
  }

  get agent(): IyonAgent { return this.dependencies.agent; }
  get core(): IyonCoreCommands { return this.dependencies.core; }
  get model(): IyonModelMetadata { return this.dependencies.model; }

  async start(tui = this.dependencies.tui): Promise<void> {
    if (this.started) return;
    this.tui = tui ?? await Tui.open({ headless: true });
    this.ownsTui = tui === undefined;
    await this.tui.render(new Scene(this.initialView(), this.history));
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

  private initialView(): ViewValue {
    return View.vertical([
      View.text("Iyon"),
      View.component(this.composer).fillWidth(),
      View.text(`${this.model.provider} · ${this.model.modelId}`).style(this.theme.footer).fillWidth(),
    ]).fillWidth().fillHeight();
  }
}

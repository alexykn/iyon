import { View } from "iyon:tui";
import type { History, TextInput, View as ViewValue } from "@iyon/runtime/tui";
import type { IyonState } from "./contracts.ts";
import { MAX_COMPOSER_ROWS } from "./composer.ts";
import type { IyonTheme } from "./theme.ts";

export interface IyonViewOptions { readonly composer: TextInput; readonly history: History; readonly state: IyonState; readonly theme: IyonTheme; }

export function footerText(state: IyonState): string {
  return [state.info.provider, state.info.modelId, `effort: ${state.info.reasoningEffort}`, state.info.status].filter((value) => value.length > 0).join(" · ");
}

export function createIyonView(options: IyonViewOptions): ViewValue {
  const composer = View.component(options.composer).style(options.theme.composer).fillWidth().clampRows(MAX_COMPOSER_ROWS);
  const footer = View.text(footerText(options.state)).style(options.theme.footer).fillWidth();
  const working = options.state.working ? View.text("Working…").style(options.theme.muted).fillWidth() : View.spacer(0);
  const approval = options.state.pendingApproval === undefined ? View.spacer(0) : View.text(`Approve ${options.state.pendingApproval.toolName}?`).fillWidth();
  return View.vertical([View.text("Iyon"), working, approval, composer, footer]).fillWidth().fillHeight();
}

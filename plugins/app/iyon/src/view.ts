import { Insets, View } from "iyon:tui";
import type { History, TextInput, View as ViewValue, WorkingActivityHandle } from "@iyon/runtime/tui";
import type { IyonState } from "./contracts.ts";
import { MAX_COMPOSER_ROWS } from "./composer.ts";
import { approvalView } from "./approvals.ts";
import { hasActiveWork } from "./state.ts";
import type { IyonTheme } from "./theme.ts";

export interface IyonViewOptions { readonly composer: TextInput; readonly history: History; readonly state: IyonState; readonly theme: IyonTheme; readonly working?: WorkingActivityHandle; }

export function footerText(state: IyonState): string {
  const effort = { none: "None", minimal: "Minimal", low: "Low", medium: "Medium", high: "High", xhigh: "XHigh", max: "Max" }[state.info.reasoningEffort];
  return [state.info.provider, state.info.modelId, `effort: ${effort}`, state.info.status].filter((value) => value.length > 0).join(" · ");
}

export function createIyonView(options: IyonViewOptions): ViewValue {
  if (options.state.goodbye) return View.spacer(0);
  const composer = View.component(options.composer)
    .style(options.theme.composer)
    .styleState("iyon.agent.effort", options.state.info.reasoningEffort)
    .fillWidth();
  const footer = View.text(footerText(options.state)).style(options.theme.footer).fillWidth();
  const working = hasActiveWork(options.state) && options.working !== undefined
    ? View.component(options.working).fillWidth().padding(Insets.of(0, 0, 1, 0))
    : View.spacer(0);
  const approval = options.state.pendingApproval === undefined ? View.spacer(0) : approvalView(options.state.pendingApproval);
  return View.vertical((column) => {
    column.child(working);
    column.child(approval);
    column.contentMax(MAX_COMPOSER_ROWS, composer);
    column.child(footer);
  }).fillWidth().fillHeight();
}

export function userBatchView(messages: readonly string[], theme: IyonTheme): ViewValue {
  return View.vertical(messages.map((message) => View.text(message).fillWidth()))
    .fillWidth()
    .border({ style: "plain", edges: "topBottom", color: theme.inputBorder });
}

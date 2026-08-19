import { Insets, View } from "iyon:tui";
import type { History, TextInput, View as ViewValue, ViewSlot } from "@iyon/runtime/tui";
import type { IyonState } from "./contracts.ts";
import { MAX_COMPOSER_ROWS } from "./composer.ts";
import { approvalView } from "./approvals.ts";
import type { IyonTheme } from "./theme.ts";

export interface IyonViewOptions { readonly composer: TextInput; readonly history: History; readonly state: IyonState; readonly theme: IyonTheme; readonly working?: ViewSlot; }


const SPINNER_FRAMES = ["⠋⣠", "⢁⡴", "⣠⠞", "⡴⠋", "⠞⢁"] as const;

export function workingFrames(waiting: boolean): ViewValue[] {
  return SPINNER_FRAMES.map((frame, index) => {
    const spinner = waiting ? frame : SPINNER_FRAMES[SPINNER_FRAMES.length - 1 - index];
    return View.text(`${spinner} ${waiting ? "waiting" : "Working"}`).noWrap();
  });
}

export function workingQueueView(state: IyonState, theme: IyonTheme): ViewValue | undefined {
  const first = state.steering[0];
  if (first === undefined) return undefined;
  const preview = first.split(/\s+/).filter(Boolean).join(" ");
  const extra = state.steering.length - 1;
  const muted = (text: string) => View.text(text).noWrap().italic().foreground(theme.mutedColor);
  return View.horizontal((row) => {
    row.flex(muted(`Queue: ${preview}`));
    if (extra > 0) row.child(muted(` + ${extra} more`));
  });
}

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
  const working = options.state.activityVisible && options.working !== undefined
    ? View.horizontal((row) => {
      row.gap(4);
      row.child(View.component(options.working!));
      const queue = workingQueueView(options.state, options.theme);
      if (queue !== undefined) row.flex(queue);
    }).fillWidth().padding(Insets.of(0, 2, 1, 2))
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

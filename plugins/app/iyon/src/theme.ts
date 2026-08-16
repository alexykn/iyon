import { Style } from "iyon:tui";
import type { StyleSpec } from "@iyon/runtime/tui";

export interface IyonTheme {
  readonly composer: StyleSpec;
  readonly footer: StyleSpec;
  readonly muted: StyleSpec;
  readonly inputBorder: "theme:input.border";
  readonly mutedColor: "theme:text.muted";
  readonly toolFinishedColor: "theme:tool.finished";
}

export function createIyonTheme(): IyonTheme {
  return {
    composer: Style.new(),
    footer: Style.new().dim(),
    muted: Style.new().dim().foreground("theme:text.muted"),
    inputBorder: "theme:input.border",
    mutedColor: "theme:text.muted",
    toolFinishedColor: "theme:tool.finished",
  };
}

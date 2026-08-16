import { Style } from "iyon:tui";
import type { StyleSpec } from "@iyon/runtime/tui";

export interface IyonTheme {
  readonly composer: StyleSpec;
  readonly footer: StyleSpec;
  readonly muted: StyleSpec;
}

export function createIyonTheme(): IyonTheme {
  return { composer: Style.new(), footer: Style.new().dim(), muted: Style.new().dim() };
}

import { Style, StyleSelector, TextSelector, Theme, themeColor } from "@iyon/tui";
import type {
  AnsiColor,
  StyleSpec,
  Theme as RuntimeTheme,
  ThemeColor,
  ThemeColorReference,
} from "@iyon/tui";

export type IyonTheme = RuntimeTheme & {
  readonly composer: StyleSpec;
  readonly footer: StyleSpec;
  readonly muted: StyleSpec;
  readonly inputBorder: ThemeColorReference;
  readonly mutedColor: ThemeColorReference;
  readonly toolFinishedColor: ThemeColorReference;
};

function rgb(r: number, g: number, b: number): ThemeColor {
  return { type: "rgb", r, g, b };
}

function ansi(value: AnsiColor): ThemeColor {
  return { type: "named", value };
}

function effort(value: string): StyleSelector {
  return StyleSelector.state("iyon.agent.effort", value);
}

function focusedEffort(value: string): StyleSelector {
  return StyleSelector.focused().andState("iyon.agent.effort", value);
}

export function createIyonTheme(effortOverride?: string): IyonTheme {
  const currentEffort = effortOverride ?? "medium";
  const accentColor =
    currentEffort === "none" ? ansi("white")
    : currentEffort === "minimal" ? ansi("lightCyan")
    : currentEffort === "low" ? ansi("green")
    : currentEffort === "medium" ? rgb(255, 196, 87)
    : currentEffort === "high" ? ansi("magenta")
    : currentEffort === "xhigh" ? ansi("lightMagenta")
    : currentEffort === "max" ? ansi("lightRed")
    : rgb(255, 196, 87);
  // text.code stays constant — only text.heading changes with effort
  const codeColor = rgb(120, 200, 210);
  const diffHeaderColor = rgb(255, 196, 87);

  // Border color for each effort level (used as base + for frozen messages).
  const inputBorderColor =
    currentEffort === "none" ? ansi("white")
    : currentEffort === "minimal" ? ansi("lightCyan")
    : currentEffort === "low" ? ansi("green")
    : currentEffort === "medium" ? ansi("yellow")
    : currentEffort === "high" ? ansi("magenta")
    : currentEffort === "xhigh" ? ansi("lightMagenta")
    : currentEffort === "max" ? ansi("lightRed")
    : rgb(173, 216, 230);
  const theme = Theme.new()
    .withColor("surface.user", rgb(45, 55, 72))
    .withColor("text.muted", rgb(113, 128, 150))
    .withColor("surface.default", rgb(113, 128, 150))
    .withColor("tool.running", rgb(160, 174, 192))
    .withColor("tool.finished", rgb(104, 211, 145))
    .withColor("tool.error", ansi("red"))
    .withColor("text.error", ansi("red"))
    .withColor("text.warning", ansi("yellow"))
    .withColor("text.heading", accentColor)
    .withColor("text.code", codeColor)
    .withColor("diff.addition", rgb(104, 211, 145))
    .withColor("diff.deletion", ansi("red"))
    .withColor("diff.header", diffHeaderColor)
    .withColor("diff.context", rgb(113, 128, 150))
    .withColor("diff.meta", rgb(113, 128, 150))
    .withColor("truncation_footer", rgb(120, 122, 132))
    .withColor("input.border", inputBorderColor)
    .withColorVariant("input.border", effort("none"), ansi("white"))
    .withColorVariant("input.border", effort("minimal"), ansi("lightCyan"))
    .withColorVariant("input.border", effort("low"), ansi("green"))
    .withColorVariant("input.border", effort("medium"), ansi("yellow"))
    .withColorVariant("input.border", effort("high"), ansi("magenta"))
    .withColorVariant("input.border", effort("xhigh"), ansi("lightMagenta"))
    .withColorVariant("input.border", effort("max"), ansi("lightRed"))
    .withColorVariant("input.border", focusedEffort("none"), ansi("white"))
    .withColorVariant("input.border", focusedEffort("minimal"), ansi("lightCyan"))
    .withColorVariant("input.border", focusedEffort("low"), ansi("lightGreen"))
    .withColorVariant("input.border", focusedEffort("medium"), ansi("lightYellow"))
    .withColorVariant("input.border", focusedEffort("high"), ansi("lightMagenta"))
    .withColorVariant("input.border", focusedEffort("xhigh"), ansi("lightMagenta"))
    .withColorVariant("input.border", focusedEffort("max"), ansi("lightRed"))
    .withStyle("tool.running", Style.new().foreground(themeColor("tool.running")))
    .withStyle("tool.finished", Style.new().foreground(themeColor("tool.finished")))
    .withStyle("tool.error", Style.new().foreground(themeColor("tool.error")))
    .withStyle("text.muted", Style.new().foreground(themeColor("text.muted")))
    .withStyle("text.error", Style.new().foreground(themeColor("text.error")))
    .withStyle("text.warning", Style.new().foreground(themeColor("text.warning")))
    .withStyle("diff.addition", Style.new().foreground(themeColor("diff.addition")))
    .withStyle("diff.deletion", Style.new().foreground(themeColor("diff.deletion")))
    .withStyle("diff.header", Style.new().foreground(themeColor("diff.header")))
    .withStyle("diff.context", Style.new().foreground(themeColor("diff.context")))
    .withStyle("diff.meta", Style.new().foreground(themeColor("diff.meta")))
    .withTextStyle(TextSelector.heading(), Style.new().foreground(themeColor("text.heading")))
    .withTextStyle(TextSelector.inlineCode(), Style.new().foreground(themeColor("text.code")))
    .withTextStyle(TextSelector.codeBlock(), Style.new().foreground(themeColor("text.code")))
    .withTextStyle(TextSelector.part("codeLabel"), Style.new().foreground(themeColor("text.muted")).dim())
    .withTextStyle(TextSelector.part("quoteMarker"), Style.new().foreground(themeColor("text.muted")))
    .withTextStyle(TextSelector.part("listMarker"), Style.new().foreground(themeColor("text.muted")))
    .withTextStyle(TextSelector.part("taskMarker"), Style.new().foreground(themeColor("text.muted")))
    .withTextStyle(TextSelector.part("thematicRule"), Style.new().foreground(themeColor("text.muted")))
    .withTextStyle(TextSelector.annotation("app", "thinking"), Style.new().foreground(themeColor("text.muted")).italic());

  return Object.assign(theme, {
    composer: Style.new(),
    footer: Style.new().dim(),
    muted: Style.new().dim().foreground(themeColor("text.muted")),
    inputBorder: themeColor("input.border"),
    mutedColor: themeColor("text.muted"),
    toolFinishedColor: themeColor("tool.finished"),
  });
}

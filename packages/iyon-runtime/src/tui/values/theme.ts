import type { ColorNode, StyleNode } from "../ir.ts";
import { StyleSpec } from "./style.ts";

export class ThemeKey {
  readonly kind = "theme-key" as const;
  constructor(readonly value: string) { if (value.length === 0) throw new RangeError("theme key cannot be empty"); }
}

export class Theme {
  readonly kind = "theme" as const;
  private constructor(private readonly styles: ReadonlyMap<string, StyleNode>, private readonly colors: ReadonlyMap<string, ColorNode>) {}
  static new(): Theme { return new Theme(new Map(), new Map()); }
  withStyle(key: string | ThemeKey, style: StyleSpec): Theme { const values = new Map(this.styles); values.set(themeKey(key), style.value); return new Theme(values, this.colors); }
  withColor(key: string | ThemeKey, color: ColorNode): Theme { const values = new Map(this.colors); values.set(themeKey(key), color); return new Theme(this.styles, values); }
  style(key: string | ThemeKey): StyleSpec { return new StyleSpec(this.styles.get(themeKey(key)) ?? { attributes: {} }); }
  color(key: string | ThemeKey): ColorNode | undefined { return this.colors.get(themeKey(key)); }
}

function themeKey(key: string | ThemeKey): string { return typeof key === "string" ? key : key.value; }

import type { NativeHandleId } from "./types.ts";

export type ColorNode = string | { readonly type: "ansi"; readonly value: number };

export interface InsetsNode {
  readonly top: number;
  readonly right: number;
  readonly bottom: number;
  readonly left: number;
}

export interface StyleNode {
  readonly foreground?: ColorNode;
  readonly background?: ColorNode;
  readonly attributes: Readonly<Record<string, boolean>>;
}

export type ViewNode =
  | { readonly type: "text"; readonly spans: readonly TextSpanNode[]; readonly wrap: string; readonly align: string }
  | { readonly type: "spacer"; readonly rows: number }
  | { readonly type: "row" | "column"; readonly children: readonly ViewNode[]; readonly gap: number }
  | { readonly type: "hanging"; readonly prefix: ViewNode; readonly continuation: ViewNode; readonly body: ViewNode }
  | { readonly type: "grid"; readonly children: readonly ViewNode[] }
  | { readonly type: "container" | "clamp"; readonly child: ViewNode; readonly maxRows?: number }
  | { readonly type: "component"; readonly handle: NativeHandleId }
  | { readonly type: "decorated"; readonly child: ViewNode; readonly decoration: DecorationNode };

export interface TextSpanNode {
  readonly text: string;
  readonly style?: StyleNode;
}

export interface DecorationNode {
  readonly padding?: InsetsNode;
  readonly background?: ColorNode;
  readonly foreground?: ColorNode;
  readonly border?: BorderNode;
  readonly style: StyleNode;
  readonly width?: "fit" | "fill";
  readonly height?: "fit" | "fill";
  readonly minWidth?: number;
  readonly maxWidth?: number;
  readonly minHeight?: number;
  readonly maxHeight?: number;
}

export interface BorderNode {
  readonly glyphs?: Readonly<Record<string, string>>;
  readonly style?: string;
}

export function emptyStyle(): StyleNode {
  return { attributes: {} };
}

export function emptyDecoration(): DecorationNode {
  return { style: emptyStyle() };
}

export function cloneDecoration(decoration: DecorationNode): DecorationNode {
  return {
    ...decoration,
    padding: decoration.padding === undefined ? undefined : { ...decoration.padding },
    style: { ...decoration.style, attributes: { ...decoration.style.attributes } },
    border: decoration.border === undefined ? undefined : { ...decoration.border, glyphs: decoration.border.glyphs && { ...decoration.border.glyphs } },
  };
}

export function mergeStyles(left: StyleNode, right: StyleNode): StyleNode {
  return {
    foreground: right.foreground ?? left.foreground,
    background: right.background ?? left.background,
    attributes: { ...left.attributes, ...right.attributes },
  };
}

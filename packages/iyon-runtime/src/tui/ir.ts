import type { NativeHandleId } from "./types.ts";

export type ColorNode = string | { readonly type: "ansi"; readonly value: number };

export interface InsetsNode {
  readonly top: number;
  readonly right: number;
  readonly bottom: number;
  readonly left: number;
}

export interface StyleNode {
  readonly theme?: string;
  readonly foreground?: ColorNode;
  readonly background?: ColorNode;
  readonly attributes: Readonly<Record<string, boolean>>;
}

export type OverflowIndicatorNode =
  | { readonly kind: "none" }
  | { readonly kind: "ellipsis"; readonly style: StyleNode }
  | { readonly kind: "footer"; readonly prefix: string; readonly style: StyleNode };

export type ViewNode =
  | { readonly type: "text"; readonly spans: readonly TextSpanNode[]; readonly wrap: string; readonly align: string }
  | { readonly type: "diff"; readonly hunks: readonly DiffHunkNode[] }
  | { readonly type: "spacer"; readonly rows: number }
  | { readonly type: "row" | "column"; readonly children: readonly LayoutChild[]; readonly gap: number }
  | { readonly type: "hanging"; readonly prefix: ViewNode; readonly continuation: ViewNode; readonly body: ViewNode }
  | { readonly type: "grid"; readonly columns: readonly GridTrackNode[]; readonly rows: readonly GridRowNode[]; readonly columnGap: number; readonly rowGap: number }
  | { readonly type: "container" | "clamp"; readonly child: ViewNode; readonly maxRows?: number; readonly overflow?: OverflowIndicatorNode }
  | { readonly type: "contentMax"; readonly child: ViewNode; readonly maxRows: number }
  | { readonly type: "component"; readonly handle: NativeHandleId }
  | { readonly type: "decorated"; readonly child: ViewNode; readonly decoration: DecorationNode };

export interface TextSpanNode {
  readonly text: string;
  readonly style?: StyleNode;
}

export interface DiffRangeNode {
  readonly start: number;
  readonly count: number;
}

export interface DiffLineNode {
  readonly kind: "context" | "addition" | "deletion";
  readonly text: string;
  readonly termination: "terminated" | "unterminated";
  readonly oldLine?: number;
  readonly newLine?: number;
}

export interface DiffHunkNode {
  readonly oldRange: DiffRangeNode;
  readonly newRange: DiffRangeNode;
  readonly lines: readonly DiffLineNode[];
}

export type LayoutChild =
  | { readonly kind: "normal"; readonly child: ViewNode }
  | { readonly kind: "fixed"; readonly size: number; readonly child: ViewNode }
  | { readonly kind: "flex"; readonly child: ViewNode }
  | { readonly kind: "flexMax"; readonly maxRows: number; readonly child: ViewNode }
  | { readonly kind: "contentMax"; readonly maxRows: number; readonly child: ViewNode };

export type GridTrackNode =
  | { readonly kind: "content" }
  | { readonly kind: "contentMax"; readonly max: number }
  | { readonly kind: "fixed"; readonly size: number }
  | { readonly kind: "flex" }
  | { readonly kind: "flexMax"; readonly max: number };

export interface GridCellNode {
  readonly view: ViewNode;
  readonly columnSpan: number;
  readonly rowSpan: number;
  readonly horizontalAlign: "start" | "center" | "end";
  readonly verticalAlign: "top" | "center" | "bottom";
}

export interface GridRowNode {
  readonly track: GridTrackNode;
  readonly cells: readonly GridCellNode[];
}

export interface DecorationNode {
  readonly padding?: InsetsNode;
  readonly background?: ColorNode;
  readonly foreground?: ColorNode;
  readonly border?: BorderNode;
  readonly style: StyleNode;
  readonly styleStates?: Readonly<Record<string, string>>;
  readonly width?: "fit" | "fill";
  readonly height?: "fit" | "fill";
  readonly minWidth?: number;
  readonly maxWidth?: number;
  readonly minHeight?: number;
  readonly maxHeight?: number;
}

export interface BorderNode {
  readonly glyphs?: Readonly<Record<string, string>>;
  readonly style?: "plain" | "rounded" | "double";
  readonly edges?: "all" | "topBottom";
  readonly color?: ColorNode;
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

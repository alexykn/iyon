import type { NativeHandleId } from "./types.ts";

/** Private semantic bridge schema shared by the retained TS DAG and native decoder. */
export const VIEW_BRIDGE_SCHEMA_VERSION = 1 as const;

export const BRIDGE_VIEW_KIND = {
  text: 1,
  diff: 2,
  spacer: 3,
  row: 4,
  column: 5,
  hanging: 6,
  grid: 7,
  container: 8,
  clamp: 9,
  contentMax: 10,
  component: 11,
  decorated: 12,
} as const;

export const BRIDGE_LAYOUT_CHILD_KIND = {
  normal: 1,
  fixed: 2,
  flex: 3,
  flexMax: 4,
  contentMax: 5,
} as const;

export const BRIDGE_GRID_TRACK_KIND = {
  content: 1,
  contentMax: 2,
  fixed: 3,
  flex: 4,
  flexMax: 5,
} as const;

export const BRIDGE_OVERFLOW_KIND = {
  none: 1,
  ellipsis: 2,
  footer: 3,
} as const;

export const BRIDGE_WRAP_MODE = {
  wordThenGrapheme: 1,
  grapheme: 2,
  noWrap: 3,
} as const;

export const BRIDGE_HORIZONTAL_ALIGN = {
  start: 1,
  center: 2,
  end: 3,
} as const;

export const BRIDGE_VERTICAL_ALIGN = {
  top: 1,
  center: 2,
  bottom: 3,
} as const;

export const BRIDGE_DIFF_LINE_KIND = {
  context: 1,
  addition: 2,
  deletion: 3,
} as const;

export const BRIDGE_DIFF_LINE_TERMINATION = {
  terminated: 1,
  unterminated: 2,
} as const;

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

/** Internal numeric representation. It is never exported from the public TUI entrypoint. */
export type BridgeDiffLineNode = {
  readonly kind: (typeof BRIDGE_DIFF_LINE_KIND)[keyof typeof BRIDGE_DIFF_LINE_KIND];
  readonly text: string;
  readonly termination: (typeof BRIDGE_DIFF_LINE_TERMINATION)[keyof typeof BRIDGE_DIFF_LINE_TERMINATION];
  readonly oldLine?: number;
  readonly newLine?: number;
};

export interface BridgeDiffHunkNode {
  readonly oldRange: DiffRangeNode;
  readonly newRange: DiffRangeNode;
  readonly lines: readonly BridgeDiffLineNode[];
}

export type BridgeOverflowIndicatorNode =
  | { readonly kind: typeof BRIDGE_OVERFLOW_KIND.none }
  | { readonly kind: typeof BRIDGE_OVERFLOW_KIND.ellipsis; readonly style: StyleNode }
  | { readonly kind: typeof BRIDGE_OVERFLOW_KIND.footer; readonly prefix: string; readonly style: StyleNode };

export type BridgeLayoutChild =
  | { readonly kind: typeof BRIDGE_LAYOUT_CHILD_KIND.normal; readonly child: BridgeViewNode }
  | { readonly kind: typeof BRIDGE_LAYOUT_CHILD_KIND.fixed; readonly size: number; readonly child: BridgeViewNode }
  | { readonly kind: typeof BRIDGE_LAYOUT_CHILD_KIND.flex; readonly child: BridgeViewNode }
  | { readonly kind: typeof BRIDGE_LAYOUT_CHILD_KIND.flexMax; readonly maxRows: number; readonly child: BridgeViewNode }
  | { readonly kind: typeof BRIDGE_LAYOUT_CHILD_KIND.contentMax; readonly maxRows: number; readonly child: BridgeViewNode };

export type BridgeGridTrackNode =
  | { readonly kind: typeof BRIDGE_GRID_TRACK_KIND.content }
  | { readonly kind: typeof BRIDGE_GRID_TRACK_KIND.contentMax; readonly max: number }
  | { readonly kind: typeof BRIDGE_GRID_TRACK_KIND.fixed; readonly size: number }
  | { readonly kind: typeof BRIDGE_GRID_TRACK_KIND.flex }
  | { readonly kind: typeof BRIDGE_GRID_TRACK_KIND.flexMax; readonly max: number };

export interface BridgeGridCellNode {
  readonly view: BridgeViewNode;
  readonly columnSpan: number;
  readonly rowSpan: number;
  readonly horizontalAlign: number;
  readonly verticalAlign: number;
}

export interface BridgeGridRowNode {
  readonly track: BridgeGridTrackNode;
  readonly cells: readonly BridgeGridCellNode[];
}

type BridgeViewNodeData =
  | { readonly kind: typeof BRIDGE_VIEW_KIND.text; readonly spans: readonly TextSpanNode[]; readonly wrap: number; readonly align: number }
  | { readonly kind: typeof BRIDGE_VIEW_KIND.diff; readonly hunks: readonly BridgeDiffHunkNode[] }
  | { readonly kind: typeof BRIDGE_VIEW_KIND.spacer; readonly rows: number }
  | { readonly kind: typeof BRIDGE_VIEW_KIND.row | typeof BRIDGE_VIEW_KIND.column; readonly children: readonly BridgeLayoutChild[]; readonly gap: number }
  | { readonly kind: typeof BRIDGE_VIEW_KIND.hanging; readonly prefix: BridgeViewNode; readonly continuation: BridgeViewNode; readonly body: BridgeViewNode }
  | { readonly kind: typeof BRIDGE_VIEW_KIND.grid; readonly columns: readonly BridgeGridTrackNode[]; readonly rows: readonly BridgeGridRowNode[]; readonly columnGap: number; readonly rowGap: number }
  | { readonly kind: typeof BRIDGE_VIEW_KIND.container | typeof BRIDGE_VIEW_KIND.clamp; readonly child: BridgeViewNode; readonly maxRows?: number; readonly overflow?: BridgeOverflowIndicatorNode }
  | { readonly kind: typeof BRIDGE_VIEW_KIND.contentMax; readonly child: BridgeViewNode; readonly maxRows: number }
  | { readonly kind: typeof BRIDGE_VIEW_KIND.component; readonly handle: NativeHandleId }
  | { readonly kind: typeof BRIDGE_VIEW_KIND.decorated; readonly child: BridgeViewNode; readonly decoration: DecorationNode };

export type BridgeViewNodeDraft = BridgeViewNodeData;
export type BridgeViewNode = BridgeViewNodeData & {
  readonly id: number;
  readonly schema: typeof VIEW_BRIDGE_SCHEMA_VERSION;
};

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

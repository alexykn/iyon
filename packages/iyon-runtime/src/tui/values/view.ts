import type { NativeHandleId } from "../types.ts";
import {
  BRIDGE_DIFF_LINE_KIND,
  BRIDGE_DIFF_LINE_TERMINATION,
  BRIDGE_GRID_TRACK_KIND,
  BRIDGE_HORIZONTAL_ALIGN,
  BRIDGE_LAYOUT_CHILD_KIND,
  BRIDGE_OVERFLOW_KIND,
  BRIDGE_VIEW_KIND,
  BRIDGE_VERTICAL_ALIGN,
  BRIDGE_WRAP_MODE,
  cloneDecoration,
  cloneStyle,
  emptyDecoration,
  emptyStyle,
  mergeStyles,
  type BorderNode,
  type BridgeDiffHunkNode,
  type BridgeGridCellNode,
  type BridgeGridRowNode,
  type BridgeGridTrackNode,
  type BridgeLayoutChild,
  type BridgeOverflowIndicatorNode,
  type BridgeViewNode,
  type BridgeViewNodeDraft,
  type ColorNode,
  type TextSpanNode,
  type DecorationNode,
  type DiffHunkNode,
  type DiffLineNode,
  type GridTrackNode,
  type InsetsNode,
  type OverflowIndicatorNode,
  type StyleNode,
  VIEW_BRIDGE_SCHEMA_VERSION,
} from "../ir.ts";
import { insets, Insets } from "./geometry.ts";
import { StyleSpec } from "./style.ts";
import { TextSpan, type HorizontalAlign, type WrapMode } from "./text.ts";
import { packedMeta, registerPackedMeta, setPackedGridCells, setPackedSequence, type PackedGridCell, type PackedLineage, type PackedMetaSeed } from "../packed_v3_meta.ts";
import { PersistentSeq } from "../persistent_seq.ts";

type ChildBuilder = readonly View[] | ((builder: ChildrenBuilder) => void);
type CounterBox = { next: number };

type PendingCreateKind = "text" | "spacer" | "axis";
type PendingPatchKind = "textLayout" | "common";

type PendingAxisChild = Readonly<{
  view: View;
  kind: BridgeLayoutChild["kind"];
  value?: number;
}>;

/**
 * Stable-shape private semantic backing. Pending values carry only the compact
 * recipe needed by the generated/native route; BridgeViewNode is materialized
 * lazily by nodeForBridge for the cold/direct compatibility path.
 */
interface ViewBacking {
  readonly state: 0 | 1 | 2;
  readonly nodeId: number;
  readonly nodeIdLow: number;
  readonly nodeIdHigh: number;
  readonly nodeIdPair: readonly [number, number];
  readonly node?: BridgeViewNode;
  readonly createKind?: PendingCreateKind;
  readonly spans?: readonly TextSpanNode[];
  readonly rows?: number;
  readonly axisHorizontal?: boolean;
  readonly axisGap?: number;
  readonly axisChildren?: readonly PendingAxisChild[];
  readonly wrap?: number;
  readonly align?: number;
  readonly patchKind?: PendingPatchKind;
  readonly base?: View;
  readonly mask?: number;
  readonly paddingTopRight?: number;
  readonly paddingBottomLeft?: number;
  readonly widthRule?: number;
  readonly heightRule?: number;
  readonly minWidth?: number;
  readonly maxWidth?: number;
  readonly minHeight?: number;
  readonly maxHeight?: number;
}

const PATCH_PADDING = 4;
const PATCH_WIDTH = 8;
const PATCH_HEIGHT = 16;
const PATCH_MIN_WIDTH = 32;
const PATCH_MAX_WIDTH = 64;
const PATCH_MIN_HEIGHT = 128;
const PATCH_MAX_HEIGHT = 256;
const kBacking = Symbol("iyon:tui:view-backing");

export interface NativeTextLayoutPatch {
  readonly kind: "textLayout";
  readonly base: View;
  readonly wrap: number;
  readonly align: number;
}

export interface NativeCommonPatch {
  readonly kind: "common";
  readonly base: View;
  readonly mask: number;
  readonly paddingTopRight: number;
  readonly paddingBottomLeft: number;
  readonly widthRule: number;
  readonly heightRule: number;
  readonly minWidth: number;
  readonly maxWidth: number;
  readonly minHeight: number;
  readonly maxHeight: number;
}

export type NativeScalarPatch = NativeTextLayoutPatch | NativeCommonPatch;

/** Native retained-path metadata; it stores selectors, never a View graph. */
export interface NativePathStep {
  readonly kind: number;
  readonly expectedViewKind: number;
  readonly selector: number;
}

export interface NativePathLineage {
  /** Full JS semantic NodeId of the previous root; no View is retained. */
  readonly baseNodeId: number;
  readonly parent?: NativePathLineage;
  readonly step?: NativePathStep;
  readonly depth: number;
}

export const NATIVE_PATH_VIEW_KIND = Object.freeze({
  text: 1,
  row: 2,
  column: 3,
  grid: 4,
  hanging: 5,
  container: 6,
  clampRows: 7,
  rowViewport: 8,
});

export const NATIVE_PATH_STEP = Object.freeze({
  containerChild: 1,
  clampChild: 2,
  rowViewportChild: 3,
  columnChild: 4,
  rowChild: 5,
  gridCell: 6,
  hangingPrefix: 7,
  hangingContinuation: 8,
  hangingBody: 9,
});
const NODE_ID_COUNTER = Symbol.for("iyon:tui:private-view-node-counter");
const globalRoot = globalThis as typeof globalThis & { [NODE_ID_COUNTER]?: CounterBox };
const nodeIdCounter = globalRoot[NODE_ID_COUNTER] ??= { next: 1 };

function nextNodeId(): number {
  if (nodeIdCounter.next > Number.MAX_SAFE_INTEGER) throw new Error("TUI View node identity exhausted");
  return nodeIdCounter.next++;
}

function makeBacking(
  state: 0 | 1 | 2,
  nodeId: number,
  fields: Partial<ViewBacking> = {},
): ViewBacking {
  return Object.freeze({
    state,
    nodeId,
    nodeIdLow: nodeId >>> 0,
    nodeIdHigh: Math.floor(nodeId / 0x1_0000_0000),
    nodeIdPair: Object.freeze([nodeId >>> 0, Math.floor(nodeId / 0x1_0000_0000)] as const),
    node: fields.node,
    createKind: fields.createKind,
    spans: fields.spans,
    rows: fields.rows,
    axisHorizontal: fields.axisHorizontal,
    axisGap: fields.axisGap,
    axisChildren: fields.axisChildren,
    wrap: fields.wrap,
    align: fields.align,
    patchKind: fields.patchKind,
    base: fields.base,
    mask: fields.mask,
    paddingTopRight: fields.paddingTopRight,
    paddingBottomLeft: fields.paddingBottomLeft,
    widthRule: fields.widthRule,
    heightRule: fields.heightRule,
    minWidth: fields.minWidth,
    maxWidth: fields.maxWidth,
    minHeight: fields.minHeight,
    maxHeight: fields.maxHeight,
  });
}

function pendingTextBacking(spans: readonly TextSpanNode[], wrap: number, align: number): ViewBacking {
  return makeBacking(1, nextNodeId(), {
    createKind: "text",
    spans,
    wrap,
    align,
  });
}

function pendingSpacerBacking(rows: number): ViewBacking {
  return makeBacking(1, nextNodeId(), { createKind: "spacer", rows });
}

function pendingAxisBacking(
  horizontal: boolean,
  gap: number,
  children: readonly PendingAxisChild[],
): ViewBacking {
  return makeBacking(1, nextNodeId(), {
    createKind: "axis",
    axisHorizontal: horizontal,
    axisGap: gap,
    axisChildren: Object.freeze(children.map((child) => Object.freeze({ ...child }))),
  });
}

function pendingTextPatchBacking(base: View, wrap: number, align: number): ViewBacking {
  return makeBacking(2, nextNodeId(), {
    patchKind: "textLayout",
    base,
    wrap,
    align,
  });
}

function pendingCommonPatchBacking(
  base: View,
  mask: number,
  paddingTopRight: number,
  paddingBottomLeft: number,
  widthRule: number,
  heightRule: number,
  minWidth: number,
  maxWidth: number,
  minHeight: number,
  maxHeight: number,
): ViewBacking {
  return makeBacking(2, nextNodeId(), {
    patchKind: "common",
    base,
    mask,
    paddingTopRight,
    paddingBottomLeft,
    widthRule,
    heightRule,
    minWidth,
    maxWidth,
    minHeight,
    maxHeight,
  });
}

export type OverflowIndicator =
  | { readonly kind: "none" }
  | { readonly kind: "ellipsis"; readonly style: StyleSpec }
  | { readonly kind: "footer"; readonly prefix: string; readonly style: StyleSpec };

export type GridTrack = GridTrackNode;

export interface GridCell {
  readonly view: View;
  readonly columnSpan?: number;
  readonly rowSpan?: number;
  readonly horizontalAlign?: "start" | "center" | "end";
  readonly verticalAlign?: "top" | "center" | "bottom";
}

export interface GridRow {
  readonly track?: GridTrack;
  readonly cells: readonly GridCell[];
}

export interface GridSpec {
  readonly columns?: readonly GridTrack[];
  readonly rows: readonly GridRow[];
  readonly columnGap?: number;
  readonly rowGap?: number;
}

export class GridRowBuilder {
  readonly cells: GridCell[] = [];
  cell(view: View): this { this.cells.push({ view }); return this; }
  cellWith(spec: Omit<GridCell, "view">, view: View): this { this.cells.push({ ...spec, view }); return this; }
}

export class GridBuilder {
  columnsValue: GridTrack[] = [];
  rows: GridRow[] = [];
  columnGapValue = 0;
  rowGapValue = 0;
  columns(columns: readonly GridTrack[]): this { this.columnsValue = [...columns]; return this; }
  columnGap(value: number): this { this.columnGapValue = validateU16(value, "columnGap"); return this; }
  rowGap(value: number): this { this.rowGapValue = validateU16(value, "rowGap"); return this; }
  row(build: ((row: GridRowBuilder) => void) | GridRow): this {
    if (typeof build === "function") {
      const row = new GridRowBuilder();
      build(row);
      this.rows.push({ cells: row.cells });
    } else this.rows.push(build);
    return this;
  }
  rowWith(track: GridTrack, build: (row: GridRowBuilder) => void): this {
    const row = new GridRowBuilder();
    build(row);
    this.rows.push({ track, cells: row.cells });
    return this;
  }
}

export class ChildrenBuilder {
  private readonly nativeChildren: PendingAxisChild[] = [];
  private layoutGap = 0;
  get children(): BridgeLayoutChild[] {
    return this.nativeChildren.map((entry) => bridgeLayoutChild(entry));
  }
  get nativeAxisChildren(): readonly PendingAxisChild[] { return this.nativeChildren; }
  child(view: View): this {
    this.nativeChildren.push({ view, kind: BRIDGE_LAYOUT_CHILD_KIND.normal });
    return this;
  }
  childrenOf(views: readonly View[]): this { for (const view of views) this.child(view); return this; }
  gap(value: number): this { this.layoutGap = validateU16(value, "gap"); return this; }
  fixed(size: number, view: View): this {
    this.nativeChildren.push({ view, kind: BRIDGE_LAYOUT_CHILD_KIND.fixed, value: validateU16(size, "size") });
    return this;
  }
  flex(view: View): this {
    this.nativeChildren.push({ view, kind: BRIDGE_LAYOUT_CHILD_KIND.flex, value: 1 });
    return this;
  }
  flexMax(maxRows: number, view: View): this {
    this.nativeChildren.push({ view, kind: BRIDGE_LAYOUT_CHILD_KIND.flexMax, value: validateU16(maxRows, "maxRows") });
    return this;
  }
  contentMax(maxRows: number, view: View): this {
    this.nativeChildren.push({ view, kind: BRIDGE_LAYOUT_CHILD_KIND.contentMax, value: validateU16(maxRows, "maxRows") });
    return this;
  }
  gapValue(): number { return this.layoutGap; }
}

function withIdentity(
  node: BridgeViewNode | BridgeViewNodeDraft,
  id: number,
  lineage?: PackedLineage,
  seed?: PackedMetaSeed,
): BridgeViewNode {
  const { id: _oldId, schema: _oldSchema, ...draft } = node as BridgeViewNode;
  const result = freezeBridgeNode({ id, schema: VIEW_BRIDGE_SCHEMA_VERSION, ...draft } as BridgeViewNode);
  registerPackedMeta(result, lineage, seed);
  return result;
}

function withPrivateIdentity(node: BridgeViewNode | BridgeViewNodeDraft, lineage?: PackedLineage, seed?: PackedMetaSeed): BridgeViewNode {
  return withIdentity(node, nextNodeId(), lineage, seed);
}

function freezeColor(color: ColorNode | undefined): void {
  if (color !== undefined && typeof color === "object") Object.freeze(color);
}

function freezeStyle(style: StyleNode): void {
  freezeColor(style.foreground);
  freezeColor(style.background);
  Object.freeze(style.attributes);
  Object.freeze(style);
}

function freezeDecoration(decoration: DecorationNode): void {
  if (decoration.padding !== undefined) Object.freeze(decoration.padding);
  freezeColor(decoration.background);
  freezeColor(decoration.foreground);
  if (decoration.border !== undefined) {
    if (decoration.border.glyphs !== undefined) Object.freeze(decoration.border.glyphs);
    freezeColor(decoration.border.color);
    Object.freeze(decoration.border);
  }
  freezeStyle(decoration.style);
  if (decoration.styleStates !== undefined) Object.freeze(decoration.styleStates);
  Object.freeze(decoration);
}

function freezeOverflow(overflow: BridgeOverflowIndicatorNode | undefined): void {
  if (overflow === undefined) return;
  if (overflow.kind !== BRIDGE_OVERFLOW_KIND.none) freezeStyle(overflow.style);
  Object.freeze(overflow);
}

function freezeDiff(hunks: readonly BridgeDiffHunkNode[]): void {
  for (const hunk of hunks) {
    Object.freeze(hunk.oldRange);
    Object.freeze(hunk.newRange);
    for (const line of hunk.lines) Object.freeze(line);
    Object.freeze(hunk.lines);
    Object.freeze(hunk);
  }
  Object.freeze(hunks);
}

function freezeBridgeNode(node: BridgeViewNode): BridgeViewNode {
  switch (node.kind) {
    case BRIDGE_VIEW_KIND.text:
      for (const span of node.spans) {
        if (span.style !== undefined) freezeStyle(span.style);
        Object.freeze(span);
      }
      Object.freeze(node.spans);
      break;
    case BRIDGE_VIEW_KIND.diff:
      freezeDiff(node.hunks);
      break;
    case BRIDGE_VIEW_KIND.row:
    case BRIDGE_VIEW_KIND.column:
      for (const child of node.children) Object.freeze(child);
      Object.freeze(node.children);
      break;
    case BRIDGE_VIEW_KIND.grid:
      for (const track of node.columns) Object.freeze(track);
      Object.freeze(node.columns);
      for (const row of node.rows) {
        Object.freeze(row.track);
        for (const cell of row.cells) Object.freeze(cell);
        Object.freeze(row.cells);
        Object.freeze(row);
      }
      Object.freeze(node.rows);
      break;
    case BRIDGE_VIEW_KIND.container:
    case BRIDGE_VIEW_KIND.contentMax:
      break;
    case BRIDGE_VIEW_KIND.clamp:
      freezeOverflow(node.overflow);
      break;
    case BRIDGE_VIEW_KIND.decorated:
      freezeDecoration(node.decoration);
      break;
    case BRIDGE_VIEW_KIND.hanging:
    case BRIDGE_VIEW_KIND.spacer:
    case BRIDGE_VIEW_KIND.component:
      break;
  }
  return Object.freeze(node);
}

export class View {
  readonly kind = "view" as const;
  readonly [kBacking]: ViewBacking;

  private constructor(
    nodeOrBacking: BridgeViewNode | BridgeViewNodeDraft | ViewBacking,
    lineage?: PackedLineage,
    seed?: PackedMetaSeed,
    nativePath?: NativePathLineage,
  ) {
    const identity = isViewBacking(nodeOrBacking) ? undefined : withPrivateIdentity(nodeOrBacking, lineage, seed);
    const backing = isViewBacking(nodeOrBacking)
      ? nodeOrBacking
      : makeBacking(0, identity!.id, { node: identity });
    this[kBacking] = backing;
    if (backing.node !== undefined) nodes.set(this, backing.node);
    if (nativePath !== undefined) nativePathLineages.set(this, nativePath);
    Object.freeze(this);
  }

  static contentMax(maxRows: number, child: View): View {
    validateU16(maxRows, "maxRows");
    return new View({ kind: BRIDGE_VIEW_KIND.contentMax, child: nodeForBridge(child), maxRows });
  }

  static diff(hunks: readonly DiffHunkNode[]): View {
    return new View({ kind: BRIDGE_VIEW_KIND.diff, hunks: hunks.map(toBridgeHunk) });
  }

  static text(value: string): View {
    if (typeof value !== "string") throw new TypeError("View.text requires a string");
    return new View(pendingTextBacking(
      Object.freeze([{ text: value }]),
      BRIDGE_WRAP_MODE.wordThenGrapheme,
      BRIDGE_HORIZONTAL_ALIGN.start,
    ));
  }

  static styledText(spans: readonly TextSpan[]): View {
    const values = spans.map((span) => ({
      ...span.value,
      style: span.value.style === undefined ? undefined : cloneStyle(span.value.style),
    }));
    Object.freeze(values);
    return new View(pendingTextBacking(
      values,
      BRIDGE_WRAP_MODE.wordThenGrapheme,
      BRIDGE_HORIZONTAL_ALIGN.start,
    ));
  }

  static spacer(rows: number): View {
    validateU16(rows, "rows");
    return new View(pendingSpacerBacking(rows));
  }

  static horizontal(children: ChildBuilder): View {
    const builder = buildChildren(children);
    return new View(pendingAxisBacking(true, builder.gapValue(), builder.nativeAxisChildren));
  }

  static vertical(children: ChildBuilder): View {
    const builder = buildChildren(children);
    return new View(pendingAxisBacking(false, builder.gapValue(), builder.nativeAxisChildren));
  }

  static hanging(prefix: View, continuation: View, body: View): View {
    return new View({ kind: BRIDGE_VIEW_KIND.hanging, prefix: nodeForBridge(prefix), continuation: nodeForBridge(continuation), body: nodeForBridge(body) });
  }

  static grid(specification: readonly View[] | GridSpec | ((builder: GridBuilder) => void)): View {
    const builder = new GridBuilder();
    if (Array.isArray(specification)) {
      builder.columns(specification.map(() => ({ kind: "content" as const })));
      builder.row((row) => specification.forEach((view) => row.cell(view)));
    } else if (typeof specification === "function") specification(builder);
    else {
      const spec = specification as GridSpec;
      builder.columns(spec.columns ?? []);
      for (const row of spec.rows) builder.row(row);
      builder.columnGap(spec.columnGap ?? 0).rowGap(spec.rowGap ?? 0);
    }
    const rows: BridgeGridRowNode[] = builder.rows.map((row) => ({
      track: bridgeGridTrack(row.track ?? { kind: "content" }),
      cells: row.cells.map((cell): BridgeGridCellNode => ({
        view: nodeForBridge(cell.view),
        columnSpan: validatePositiveU16(cell.columnSpan ?? 1, "columnSpan"),
        rowSpan: validatePositiveU16(cell.rowSpan ?? 1, "rowSpan"),
        horizontalAlign: horizontalAlignCode(cell.horizontalAlign ?? "start"),
        verticalAlign: verticalAlignCode(cell.verticalAlign ?? "top"),
      })),
    }));
    return new View({
      kind: BRIDGE_VIEW_KIND.grid,
      columns: builder.columnsValue.map(bridgeGridTrack),
      rows,
      columnGap: builder.columnGapValue,
      rowGap: builder.rowGapValue,
    });
  }

  static component(handle: { readonly id: NativeHandleId; nativeComponentId?: () => number | undefined }): View {
    const nativeId = handle.nativeComponentId?.();
    return new View({ kind: BRIDGE_VIEW_KIND.component, handle: (nativeId ?? handle.id) as NativeHandleId });
  }

  static replaceAxisChildForPackedTransport(view: View, index: number, child: View): View {
    const node = nodeForBridge(view);
    if (node.kind !== BRIDGE_VIEW_KIND.row && node.kind !== BRIDGE_VIEW_KIND.column) throw new TypeError("packed axis replacement requires a row or column");
    const current = packedMeta(node).sequence ?? PersistentSeq.from(node.children);
    const item = current.get(index);
    if (item === undefined) throw new RangeError("packed axis replacement index out of range");
    const sequence = current.set(index, { ...item, child: nodeForBridge(child) });
    const next = new View({ ...node, children: node.children }, { kind: "axis", base: node }, { sequence });
    setPackedSequence(nodeForBridge(next), sequence);
    return next;
  }

  static spliceAxisChildrenForPackedTransport(view: View, index: number, removeCount: number, children: readonly View[]): View {
    const node = nodeForBridge(view);
    if (node.kind !== BRIDGE_VIEW_KIND.row && node.kind !== BRIDGE_VIEW_KIND.column) throw new TypeError("packed axis splice requires a row or column");
    const current = packedMeta(node).sequence ?? PersistentSeq.from(node.children);
    const items = children.map((child) => ({ kind: BRIDGE_LAYOUT_CHILD_KIND.normal, child: nodeForBridge(child) }));
    const sequence = current.splice(index, removeCount, ...items);
    const next = new View({ ...node, children: node.children }, { kind: "axis", base: node }, { sequence });
    setPackedSequence(nodeForBridge(next), sequence);
    return next;
  }

  static replaceGridCellForPackedTransport(view: View, row: number, column: number, child: View): View {
    const node = nodeForBridge(view);
    if (node.kind !== BRIDGE_VIEW_KIND.grid) throw new TypeError("packed grid replacement requires a grid");
    if (!Number.isInteger(row) || row < 0 || row >= node.rows.length) throw new RangeError("packed grid row out of range");
    if (!Number.isInteger(column) || column < 0) throw new RangeError("packed grid column out of range");
    const baseMeta = packedMeta(node);
    const current = baseMeta.gridCells;
    if (current === undefined || baseMeta.gridCellOffsets === undefined) throw new Error("packed grid sequence is unavailable");
    const index = baseMeta.gridCellOffsets.get(`${row}:${column}`);
    if (index === undefined) throw new RangeError("packed grid cell index is unavailable");
    const cell = current.get(index);
    if (cell === undefined) throw new RangeError("packed grid cell out of range");
    const sequence = current.set(index, { ...cell, view: nodeForBridge(child) } satisfies PackedGridCell);
    const next = new View(
      { ...node, rows: node.rows },
      { kind: "grid", base: node },
      { gridCells: sequence, gridCellOffsets: baseMeta.gridCellOffsets },
    );
    setPackedGridCells(nodeForBridge(next), sequence);
    return next;
  }

  bold(): View { return this.textAttribute("bold"); }
  dim(): View { return this.textAttribute("dim"); }
  italic(): View { return this.textAttribute("italic"); }
  underline(): View { return this.textAttribute("underline"); }
  reversed(): View { return this.textAttribute("reversed"); }
  strikethrough(): View { return this.textAttribute("strikethrough"); }
  textAttribute(name: string, enabled = true): View { return this.decorate({ style: { ...emptyStyle(), attributes: { [name]: enabled } } }); }
  padding(value: number | Insets): View { return this.decorate({ padding: insets(value) }); }
  background(color: ColorNode): View { return this.decorate({ background: color }); }
  foreground(color: ColorNode): View { return this.decorate({ foreground: color }); }
  border(border: BorderNode): View { return this.decorate({ border }); }
  style(style: StyleSpec): View { return this.decorate({ style: mergeStyles(emptyStyle(), style.value) }); }

  styleState(key: string, value: string): View {
    if (key.length === 0 || value.length === 0) throw new RangeError("style state key and value cannot be empty");
    const decorated = this.decoratedNode();
    const current = decorated === undefined ? emptyDecoration() : cloneDecoration(decorated.decoration);
    const child = decorated?.child ?? nodeForBridge(this);
    return new View({ kind: BRIDGE_VIEW_KIND.decorated, child, decoration: { ...current, styleStates: { ...current.styleStates, [key]: value } } }, { kind: "decoration", base: nodeForBridge(this) });
  }

  container(): View { return new View({ kind: BRIDGE_VIEW_KIND.container, child: nodeForBridge(this) }); }
  clampRows(maxRows: number, overflow: OverflowIndicator = { kind: "none" }): View {
    validateU16(maxRows, "maxRows");
    return new View({ kind: BRIDGE_VIEW_KIND.clamp, child: nodeForBridge(this), maxRows, overflow: bridgeOverflow(overflow) });
  }
  fitWidth(): View { return this.decorate({ width: "fit" }); }
  fillWidth(): View { return this.decorate({ width: "fill" }); }
  fitHeight(): View { return this.decorate({ height: "fit" }); }
  fillHeight(): View { return this.decorate({ height: "fill" }); }
  minWidth(value: number): View { return this.decorate({ minWidth: validateU16(value, "minWidth") }); }
  maxWidth(value: number): View { return this.decorate({ maxWidth: validateU16(value, "maxWidth") }); }
  minHeight(value: number): View { return this.decorate({ minHeight: validateU16(value, "minHeight") }); }
  maxHeight(value: number): View { return this.decorate({ maxHeight: validateU16(value, "maxHeight") }); }
  wrap(mode: WrapMode): View { return this.textLayoutPatch(wrapCode(mode), undefined); }
  noWrap(): View { return this.wrap("noWrap"); }
  textAlign(align: HorizontalAlign): View { return this.textLayoutPatch(undefined, horizontalAlignCode(align)); }

  /** Internal retained-path constructor; not part of the public semantic API. */
  static textLayoutAtNativePathForTransport(
    view: View,
    steps: readonly NativePathStep[],
    wrap: WrapMode,
    align: HorizontalAlign,
  ): View {
    if (steps.length > 4) throw new RangeError("native retained path depth must be at most 4");
    const nextNode = patchBridgeTextPath(nodeForBridge(view), steps, wrapCode(wrap), horizontalAlignCode(align));
    let lineage: NativePathLineage = Object.freeze({ baseNodeId: nodeForBridge(view).id, depth: 0 });
    for (const step of steps) lineage = nativePathChildLineage(view, lineage, step);
    return new View(nextNode, undefined, undefined, lineage);
  }

  private decoratedNode(): Extract<BridgeViewNode, { kind: typeof BRIDGE_VIEW_KIND.decorated }> | undefined {
    const node = nodeForBridge(this);
    return node.kind === BRIDGE_VIEW_KIND.decorated ? node as Extract<BridgeViewNode, { kind: typeof BRIDGE_VIEW_KIND.decorated }> : undefined;
  }

  private decorate(decoration: Partial<DecorationNode>): View {
    const scalar = scalarCommonFields(decoration);
    if (scalar !== undefined && this[kBacking].state !== 0) {
      const existing = nativeScalarPatch(this);
      if (existing?.kind === "common") {
        return new View(pendingCommonPatchBacking(
          existing.base,
          existing.mask | scalar.mask,
          scalar.mask & PATCH_PADDING ? scalar.paddingTopRight : existing.paddingTopRight,
          scalar.mask & PATCH_PADDING ? scalar.paddingBottomLeft : existing.paddingBottomLeft,
          scalar.mask & PATCH_WIDTH ? scalar.widthRule : existing.widthRule,
          scalar.mask & PATCH_HEIGHT ? scalar.heightRule : existing.heightRule,
          scalar.mask & PATCH_MIN_WIDTH ? scalar.minWidth : existing.minWidth,
          scalar.mask & PATCH_MAX_WIDTH ? scalar.maxWidth : existing.maxWidth,
          scalar.mask & PATCH_MIN_HEIGHT ? scalar.minHeight : existing.minHeight,
          scalar.mask & PATCH_MAX_HEIGHT ? scalar.maxHeight : existing.maxHeight,
        ));
      }
      return new View(pendingCommonPatchBacking(
        this,
        scalar.mask,
        scalar.paddingTopRight,
        scalar.paddingBottomLeft,
        scalar.widthRule,
        scalar.heightRule,
        scalar.minWidth,
        scalar.maxWidth,
        scalar.minHeight,
        scalar.maxHeight,
      ));
    }
    const decorated = this.decoratedNode();
    const current = decorated === undefined ? emptyDecoration() : cloneDecoration(decorated.decoration);
    const child = decorated?.child ?? nodeForBridge(this);
    const next: DecorationNode = { ...current, ...decoration, style: decoration.style === undefined ? current.style : mergeStyles(current.style, decoration.style) };
    return new View({ kind: BRIDGE_VIEW_KIND.decorated, child, decoration: cloneDecoration(next) }, { kind: "decoration", base: nodeForBridge(this) });
  }

  private textLayoutPatch(wrap: number | undefined, align: number | undefined): View {
    const recipe = pendingTextRecipe(this);
    if (recipe !== undefined) {
      return new View(pendingTextPatchBacking(
        recipe.base,
        wrap ?? recipe.wrap,
        align ?? recipe.align,
      ));
    }
    const node = nodeForBridge(this);
    if (node.kind === BRIDGE_VIEW_KIND.text) {
      return new View({ ...node, ...(wrap === undefined ? {} : { wrap }), ...(align === undefined ? {} : { align }) }, { kind: "text", base: node });
    }
    if (node.kind === BRIDGE_VIEW_KIND.decorated && node.child.kind === BRIDGE_VIEW_KIND.text) {
      const decorated = node as Extract<BridgeViewNode, { kind: typeof BRIDGE_VIEW_KIND.decorated }>;
      return new View({ ...decorated, child: { ...decorated.child, ...(wrap === undefined ? {} : { wrap }), ...(align === undefined ? {} : { align }) } }, { kind: "text", base: node });
    }
    return this;
  }
}

function isViewBacking(value: unknown): value is ViewBacking {
  if (value === null || typeof value !== "object") return false;
  const candidate = value as Partial<ViewBacking>;
  const nodeId = candidate.nodeId;
  return (candidate.state === 0 || candidate.state === 1 || candidate.state === 2)
    && Number.isSafeInteger(nodeId)
    && nodeId !== undefined
    && nodeId > 0;
}

function pendingTextRecipe(view: View): { readonly base: View; readonly spans: readonly TextSpanNode[]; readonly wrap: number; readonly align: number } | undefined {
  const backing = view[kBacking];
  if (backing.state === 1 && backing.createKind === "text" && backing.spans !== undefined && backing.wrap !== undefined && backing.align !== undefined) {
    return { base: view, spans: backing.spans, wrap: backing.wrap, align: backing.align };
  }
  if (backing.state === 2 && backing.patchKind === "textLayout" && backing.base !== undefined && backing.wrap !== undefined && backing.align !== undefined) {
    const base = pendingTextRecipe(backing.base);
    return base === undefined ? undefined : { base: base.base, spans: base.spans, wrap: backing.wrap, align: backing.align };
  }
  return undefined;
}

function scalarCommonFields(decoration: Partial<DecorationNode>): Omit<NativeCommonPatch, "kind" | "base"> | undefined {
  if (
    decoration.background !== undefined
    || decoration.foreground !== undefined
    || decoration.border !== undefined
    || (decoration.style !== undefined && (
      decoration.style.theme !== undefined
      || decoration.style.foreground !== undefined
      || decoration.style.background !== undefined
      || Object.keys(decoration.style.attributes).length > 0
    ))
    || (decoration.styleStates !== undefined && Object.keys(decoration.styleStates).length > 0)
  ) return undefined;
  let mask = 0;
  let paddingTopRight = 0;
  let paddingBottomLeft = 0;
  if (decoration.padding !== undefined) {
    mask |= PATCH_PADDING;
    paddingTopRight = (decoration.padding.top | (decoration.padding.right << 16)) >>> 0;
    paddingBottomLeft = (decoration.padding.bottom | (decoration.padding.left << 16)) >>> 0;
  }
  if (decoration.width !== undefined) mask |= PATCH_WIDTH;
  if (decoration.height !== undefined) mask |= PATCH_HEIGHT;
  if (decoration.minWidth !== undefined) mask |= PATCH_MIN_WIDTH;
  if (decoration.maxWidth !== undefined) mask |= PATCH_MAX_WIDTH;
  if (decoration.minHeight !== undefined) mask |= PATCH_MIN_HEIGHT;
  if (decoration.maxHeight !== undefined) mask |= PATCH_MAX_HEIGHT;
  if (mask === 0) return undefined;
  return {
    mask,
    paddingTopRight,
    paddingBottomLeft,
    widthRule: decoration.width === "fit" ? 1 : decoration.width === "fill" ? 2 : 0,
    heightRule: decoration.height === "fit" ? 1 : decoration.height === "fill" ? 2 : 0,
    minWidth: decoration.minWidth ?? 0,
    maxWidth: decoration.maxWidth ?? 0,
    minHeight: decoration.minHeight ?? 0,
    maxHeight: decoration.maxHeight ?? 0,
  };
}

function materializeBacking(view: View, backing: ViewBacking): BridgeViewNode {
  if (backing.state === 0 && backing.node !== undefined) return backing.node;
  if (backing.state === 1 && backing.createKind === "text") {
    if (backing.spans === undefined || backing.wrap === undefined || backing.align === undefined) throw new TypeError("invalid pending text backing");
    return withIdentity(
      { kind: BRIDGE_VIEW_KIND.text, spans: backing.spans, wrap: backing.wrap, align: backing.align },
      backing.nodeId,
    );
  }
  if (backing.state === 1 && backing.createKind === "spacer") {
    if (backing.rows === undefined) throw new TypeError("invalid pending spacer backing");
    return withIdentity({ kind: BRIDGE_VIEW_KIND.spacer, rows: backing.rows }, backing.nodeId);
  }
  if (backing.state === 1 && backing.createKind === "axis") {
    if (backing.axisHorizontal === undefined || backing.axisGap === undefined || backing.axisChildren === undefined) {
      throw new TypeError("invalid pending axis backing");
    }
    return withIdentity({
      kind: backing.axisHorizontal ? BRIDGE_VIEW_KIND.row : BRIDGE_VIEW_KIND.column,
      children: backing.axisChildren.map(bridgeLayoutChild),
      gap: backing.axisGap,
    }, backing.nodeId);
  }
  if (backing.state === 2 && backing.patchKind === "textLayout") {
    if (backing.base === undefined || backing.wrap === undefined || backing.align === undefined) throw new TypeError("invalid pending text patch backing");
    const base = nodeForBridge(backing.base);
    if (base.kind !== BRIDGE_VIEW_KIND.text) throw new TypeError("pending text patch base is not text");
    return withIdentity(
      { ...base, wrap: backing.wrap, align: backing.align },
      backing.nodeId,
      { kind: "text", base },
    );
  }
  if (backing.state === 2 && backing.patchKind === "common") {
    if (
      backing.base === undefined
      || backing.mask === undefined
      || backing.paddingTopRight === undefined
      || backing.paddingBottomLeft === undefined
      || backing.widthRule === undefined
      || backing.heightRule === undefined
      || backing.minWidth === undefined
      || backing.maxWidth === undefined
      || backing.minHeight === undefined
      || backing.maxHeight === undefined
    ) throw new TypeError("invalid pending common patch backing");
    const decoration: DecorationNode = {
      style: emptyStyle(),
      ...(backing.mask & PATCH_PADDING ? {
        padding: Object.freeze({
          top: backing.paddingTopRight & 0xffff,
          right: backing.paddingTopRight >>> 16,
          bottom: backing.paddingBottomLeft & 0xffff,
          left: backing.paddingBottomLeft >>> 16,
        }),
      } : {}),
      ...(backing.mask & PATCH_WIDTH ? { width: backing.widthRule === 1 ? "fit" as const : "fill" as const } : {}),
      ...(backing.mask & PATCH_HEIGHT ? { height: backing.heightRule === 1 ? "fit" as const : "fill" as const } : {}),
      ...(backing.mask & PATCH_MIN_WIDTH ? { minWidth: backing.minWidth } : {}),
      ...(backing.mask & PATCH_MAX_WIDTH ? { maxWidth: backing.maxWidth } : {}),
      ...(backing.mask & PATCH_MIN_HEIGHT ? { minHeight: backing.minHeight } : {}),
      ...(backing.mask & PATCH_MAX_HEIGHT ? { maxHeight: backing.maxHeight } : {}),
    };
    const base = nodeForBridge(backing.base);
    return withIdentity(
      { kind: BRIDGE_VIEW_KIND.decorated, child: base, decoration },
      backing.nodeId,
      { kind: "decoration", base },
    );
  }
  throw new TypeError("invalid View backing");
}

const nodes = new WeakMap<View, BridgeViewNode>();
const nativePathLineages = new WeakMap<View, NativePathLineage>();

function freezeNativePathLineage(lineage: NativePathLineage): NativePathLineage {
  const parent = lineage.parent === undefined ? undefined : freezeNativePathLineage(lineage.parent);
  const step = lineage.step === undefined ? undefined : Object.freeze({ ...lineage.step });
  return Object.freeze({ baseNodeId: lineage.baseNodeId, parent, step, depth: lineage.depth });
}

/** Returns the one-time retained path lineage attached during construction. */
export function nativePathLineage(view: View): NativePathLineage | undefined {
  return nativePathLineages.get(view);
}

/** Internal construction helper used by path-aware retained tests/builders. */
export function nativePathChildLineage(
  base: View,
  parent: NativePathLineage | undefined,
  step: NativePathStep,
): NativePathLineage {
  const baseNodeId = viewNodeId(base);
  if (parent !== undefined && parent.baseNodeId !== baseNodeId) throw new Error("native path lineage base mismatch");
  const immutableStep = Object.freeze({ ...step });
  return Object.freeze({ baseNodeId, parent, step: immutableStep, depth: (parent?.depth ?? 0) + 1 });
}

/** Attaches a root/child path lineage without retaining any child View. */
export function attachNativePathLineage(view: View, lineage: NativePathLineage): void {
  if (lineage.baseNodeId === viewNodeId(view)) throw new Error("native path lineage base must be the previous root");
  nativePathLineages.set(view, freezeNativePathLineage(lineage));
}

/** Returns the full semantic NodeId without materializing a bridge node. */
export function viewNodeId(view: View): number {
  return view[kBacking].nodeId;
}

/** Internal benchmark/diagnostic view of the compact backing state. */
export function viewBackingState(view: View): 0 | 1 | 2 {
  return view[kBacking].state;
}

/** Returns the compact axis recipe used by the native builder route. */
export function nativeAxisRecipe(view: View): {
  readonly horizontal: boolean;
  readonly gap: number;
  readonly children: readonly { readonly view: View; readonly trackWord: number }[];
} | undefined {
  const backing = view[kBacking];
  if (
    backing.state !== 1
    || backing.createKind !== "axis"
    || backing.axisHorizontal === undefined
    || backing.axisGap === undefined
    || backing.axisChildren === undefined
  ) return undefined;
  return {
    horizontal: backing.axisHorizontal,
    gap: backing.axisGap,
    children: backing.axisChildren.map((child) => ({ view: child.view, trackWord: axisTrackWord(child) })),
  };
}

/** Returns a pending spacer recipe without materializing its bridge node. */
export function nativeSpacerRecipe(view: View): number | undefined {
  const backing = view[kBacking];
  return backing.state === 1 && backing.createKind === "spacer" ? backing.rows : undefined;
}

/** Returns the compact pending/native patch, if this value has one. */
export function nativeScalarPatch(view: View): NativeScalarPatch | undefined {
  const backing = view[kBacking];
  if (backing.state !== 2 || backing.base === undefined || backing.patchKind === undefined) return undefined;
  if (backing.patchKind === "textLayout") {
    if (backing.wrap === undefined || backing.align === undefined) return undefined;
    return { kind: "textLayout", base: backing.base, wrap: backing.wrap, align: backing.align };
  }
  if (
    backing.mask === undefined
    || backing.paddingTopRight === undefined
    || backing.paddingBottomLeft === undefined
    || backing.widthRule === undefined
    || backing.heightRule === undefined
    || backing.minWidth === undefined
    || backing.maxWidth === undefined
    || backing.minHeight === undefined
    || backing.maxHeight === undefined
  ) return undefined;
  return {
    kind: "common",
    base: backing.base,
    mask: backing.mask,
    paddingTopRight: backing.paddingTopRight,
    paddingBottomLeft: backing.paddingBottomLeft,
    widthRule: backing.widthRule,
    heightRule: backing.heightRule,
    minWidth: backing.minWidth,
    maxWidth: backing.maxWidth,
    minHeight: backing.minHeight,
    maxHeight: backing.maxHeight,
  };
}

/** Returns the cached u32 halves of a View's full safe-integer NodeId. */
export function nodeIdPair(view: View): readonly [number, number] {
  return view[kBacking].nodeIdPair;
}

/** Private bridge access; the retained DAG is never part of the public API. */
export function nodeForBridge(view: View): BridgeViewNode {
  const cached = nodes.get(view);
  if (cached !== undefined) return cached;
  const backing = view[kBacking];
  const node = materializeBacking(view, backing);
  nodes.set(view, node);
  return node;
}

/**
 * Builds a path-aware immutable value for retained-path differential tests and
 * future structural builders. Construction assigns a fresh NodeId to the
 * changed leaf and every rebuilt ancestor; render only passes those cached
 * scalar halves to the generated depth specialization.
 */
export function textLayoutAtNativePathForTransport(
  view: View,
  steps: readonly NativePathStep[],
  wrap: WrapMode,
  align: HorizontalAlign,
): View {
  return View.textLayoutAtNativePathForTransport(view, steps, wrap, align);
}

function patchBridgeTextPath(
  node: BridgeViewNode,
  steps: readonly NativePathStep[],
  wrap: number,
  align: number,
): BridgeViewNode {
  const step = steps[0];
  if (step === undefined) {
    if (node.kind !== BRIDGE_VIEW_KIND.text) throw new TypeError("native retained text path must terminate at text");
    return withPrivateIdentity({ ...node, wrap, align });
  }
  if (bridgePathViewKind(node.kind) !== step.expectedViewKind) {
    throw new TypeError("native retained path expected view kind does not match bridge node");
  }
  const tail = steps.slice(1);
  switch (step.kind) {
    case NATIVE_PATH_STEP.containerChild:
    case NATIVE_PATH_STEP.clampChild: {
      if (step.selector !== 0 || (node.kind !== BRIDGE_VIEW_KIND.container && node.kind !== BRIDGE_VIEW_KIND.clamp && node.kind !== BRIDGE_VIEW_KIND.contentMax)) {
        throw new RangeError("native retained single-child path is invalid");
      }
      return withPrivateIdentity({ ...node, child: patchBridgeTextPath(node.child, tail, wrap, align) });
    }
    case NATIVE_PATH_STEP.columnChild: {
      if (node.kind !== BRIDGE_VIEW_KIND.column) throw new TypeError("native retained column path kind is invalid");
      if (!Number.isInteger(step.selector) || step.selector < 0 || step.selector >= node.children.length) throw new RangeError("native retained column path selector is out of range");
      const children = node.children.map((child, index) => index === step.selector
        ? { ...child, child: patchBridgeTextPath(child.child, tail, wrap, align) }
        : child);
      return withPrivateIdentity({ ...node, children });
    }
    case NATIVE_PATH_STEP.rowChild: {
      if (node.kind !== BRIDGE_VIEW_KIND.row) throw new TypeError("native retained row path kind is invalid");
      if (!Number.isInteger(step.selector) || step.selector < 0 || step.selector >= node.children.length) throw new RangeError("native retained row path selector is out of range");
      const children = node.children.map((child, index) => index === step.selector
        ? { ...child, child: patchBridgeTextPath(child.child, tail, wrap, align) }
        : child);
      return withPrivateIdentity({ ...node, children });
    }
    case NATIVE_PATH_STEP.gridCell: {
      if (node.kind !== BRIDGE_VIEW_KIND.grid || !Number.isInteger(step.selector) || step.selector < 0) throw new TypeError("native retained grid path kind is invalid");
      let remaining = step.selector;
      let changed = false;
      const rows = node.rows.map((row) => ({
        ...row,
        cells: row.cells.map((cell) => {
          if (changed || remaining !== 0) {
            if (!changed) remaining -= 1;
            return cell;
          }
          changed = true;
          return { ...cell, view: patchBridgeTextPath(cell.view, tail, wrap, align) };
        }),
      }));
      if (!changed || remaining !== 0) throw new RangeError("native retained grid path selector is out of range");
      return withPrivateIdentity({ ...node, rows });
    }
    case NATIVE_PATH_STEP.hangingPrefix:
    case NATIVE_PATH_STEP.hangingContinuation:
    case NATIVE_PATH_STEP.hangingBody: {
      if (node.kind !== BRIDGE_VIEW_KIND.hanging || step.selector !== 0) throw new TypeError("native retained hanging path is invalid");
      const key = step.kind === NATIVE_PATH_STEP.hangingPrefix ? "prefix" : step.kind === NATIVE_PATH_STEP.hangingContinuation ? "continuation" : "body";
      return withPrivateIdentity({ ...node, [key]: patchBridgeTextPath(node[key], tail, wrap, align) });
    }
    default: throw new TypeError("unknown native retained path step");
  }
}

function bridgePathViewKind(kind: number): number {
  switch (kind) {
    case BRIDGE_VIEW_KIND.text: return NATIVE_PATH_VIEW_KIND.text;
    case BRIDGE_VIEW_KIND.row: return NATIVE_PATH_VIEW_KIND.row;
    case BRIDGE_VIEW_KIND.column: return NATIVE_PATH_VIEW_KIND.column;
    case BRIDGE_VIEW_KIND.grid: return NATIVE_PATH_VIEW_KIND.grid;
    case BRIDGE_VIEW_KIND.hanging: return NATIVE_PATH_VIEW_KIND.hanging;
    case BRIDGE_VIEW_KIND.container: return NATIVE_PATH_VIEW_KIND.container;
    case BRIDGE_VIEW_KIND.clamp:
    case BRIDGE_VIEW_KIND.contentMax: return NATIVE_PATH_VIEW_KIND.clampRows;
    default: return 0;
  }
}

/**
 * Materializes only Packed V3 sequence overrides for the legacy direct bridge.
 * Ordinary Views return their frozen node unchanged; direct decoding of a
 * retained sequence operation gets an exact array-shaped semantic object.
 */
export function nodeForDirectBridge(view: View): BridgeViewNode {
  const node = nodeForBridge(view);
  return packedMeta(node).containsSequenceOverride ? materializeDirectNode(node) : node;
}

function materializeDirectNode(node: BridgeViewNode): BridgeViewNode {
  if (node.kind === BRIDGE_VIEW_KIND.grid && packedMeta(node).sequenceOverride) {
    const sequence = packedMeta(node).gridCells;
    if (sequence === undefined) return node;
    let index = 0;
    const rows = node.rows.map((row) => ({
      ...row,
      cells: row.cells.map((cell) => {
        const next = sequence.get(index++)!;
        return { ...cell, view: materializeDirectNode(next.view) };
      }),
    }));
    return { ...node, rows };
  }
  if ((node.kind === BRIDGE_VIEW_KIND.row || node.kind === BRIDGE_VIEW_KIND.column) && packedMeta(node).sequenceOverride) {
    const sequence = packedMeta(node).sequence;
    if (sequence === undefined) return node;
    return { ...node, children: [...sequence].map((child) => ({ ...child, child: materializeDirectNode(child.child) })) };
  }
  switch (node.kind) {
    case BRIDGE_VIEW_KIND.hanging: {
      const prefix = materializeDirectNode(node.prefix);
      const continuation = materializeDirectNode(node.continuation);
      const body = materializeDirectNode(node.body);
      return prefix === node.prefix && continuation === node.continuation && body === node.body ? node : { ...node, prefix, continuation, body };
    }
    case BRIDGE_VIEW_KIND.container:
    case BRIDGE_VIEW_KIND.clamp:
    case BRIDGE_VIEW_KIND.contentMax: {
      const child = materializeDirectNode(node.child);
      return child === node.child ? node : { ...node, child };
    }
    case BRIDGE_VIEW_KIND.decorated: {
      const child = materializeDirectNode(node.child);
      return child === node.child ? node : { ...node, child };
    }
    case BRIDGE_VIEW_KIND.grid: {
      let changed = false;
      const rows = node.rows.map((row) => {
        const cells = row.cells.map((cell) => {
          const view = materializeDirectNode(cell.view);
          changed ||= view !== cell.view;
          return view === cell.view ? cell : { ...cell, view };
        });
        return cells === row.cells ? row : { ...row, cells };
      });
      return changed ? { ...node, rows } : node;
    }
    default: return node;
  }
}

/** Internal retained-transport operation: replace one axis child via a persistent sequence path. */
export function replaceAxisChildForPackedTransport(view: View, index: number, child: View): View {
  return View.replaceAxisChildForPackedTransport(view, index, child);
}

export function spliceAxisChildrenForPackedTransport(view: View, index: number, removeCount: number, children: readonly View[]): View {
  return View.spliceAxisChildrenForPackedTransport(view, index, removeCount, children);
}

export function replaceGridCellForPackedTransport(view: View, row: number, column: number, child: View): View {
  return View.replaceGridCellForPackedTransport(view, row, column, child);
}

export function textRowsForHarness(view: View): string[] { return rows(nodeForBridge(view)); }

function rows(node: BridgeViewNode): string[] {
  switch (node.kind) {
    case BRIDGE_VIEW_KIND.text: return [node.spans.map((span) => span.text).join("")];
    case BRIDGE_VIEW_KIND.diff: return node.hunks.flatMap((hunk) => [
      `@@ -${displayDiffRange(hunk.oldRange)} +${displayDiffRange(hunk.newRange)} @@`,
      ...hunk.lines.flatMap((line) => [
        `${line.kind === BRIDGE_DIFF_LINE_KIND.addition ? "+" : line.kind === BRIDGE_DIFF_LINE_KIND.deletion ? "-" : " "}${line.text}`,
        ...(line.termination === BRIDGE_DIFF_LINE_TERMINATION.unterminated ? ["\\ No newline at end of file"] : []),
      ]),
    ]);
    case BRIDGE_VIEW_KIND.spacer: return Array.from({ length: node.rows }, () => "");
    case BRIDGE_VIEW_KIND.row: return [node.children.flatMap((child) => rows(child.child)).join("")];
    case BRIDGE_VIEW_KIND.column: return node.children.flatMap((child) => rows(child.child));
    case BRIDGE_VIEW_KIND.grid: return node.rows.flatMap((row) => row.cells.flatMap((cell) => rows(cell.view)));
    case BRIDGE_VIEW_KIND.hanging: return rows(node.prefix).map((prefix, index) => `${prefix}${index === 0 ? rows(node.body)[0] ?? "" : rows(node.body)[index] ?? ""}`);
    case BRIDGE_VIEW_KIND.container: case BRIDGE_VIEW_KIND.clamp: return rows(node.child).slice(0, node.maxRows);
    case BRIDGE_VIEW_KIND.contentMax: return rows(node.child).slice(0, node.maxRows);
    case BRIDGE_VIEW_KIND.component: return [""];
    case BRIDGE_VIEW_KIND.decorated: return rows(node.child);
  }
}

function toBridgeHunk(hunk: DiffHunkNode): BridgeDiffHunkNode {
  let oldLine = hunk.oldRange.start + 1;
  let newLine = hunk.newRange.start + 1;
  const lines = hunk.lines.map((line: DiffLineNode) => {
    const node = {
      kind: BRIDGE_DIFF_LINE_KIND[line.kind],
      text: line.text,
      termination: line.termination === "unterminated" ? BRIDGE_DIFF_LINE_TERMINATION.unterminated : BRIDGE_DIFF_LINE_TERMINATION.terminated,
      ...(line.kind === "context" ? { oldLine, newLine } : {}),
      ...(line.kind === "addition" ? { newLine } : {}),
      ...(line.kind === "deletion" ? { oldLine } : {}),
    } as const;
    if (line.kind !== "addition") oldLine += 1;
    if (line.kind !== "deletion") newLine += 1;
    return node;
  });
  return { oldRange: { ...hunk.oldRange }, newRange: { ...hunk.newRange }, lines };
}

function bridgeOverflow(overflow: OverflowIndicator): BridgeOverflowIndicatorNode {
  if (overflow.kind === "none") return { kind: BRIDGE_OVERFLOW_KIND.none };
  if (overflow.kind === "ellipsis") return { kind: BRIDGE_OVERFLOW_KIND.ellipsis, style: cloneStyle(overflow.style.value) };
  return { kind: BRIDGE_OVERFLOW_KIND.footer, prefix: overflow.prefix, style: cloneStyle(overflow.style.value) };
}

function bridgeGridTrack(track: GridTrackNode): BridgeGridTrackNode {
  switch (track.kind) {
    case "content": return { kind: BRIDGE_GRID_TRACK_KIND.content };
    case "contentMax": return { kind: BRIDGE_GRID_TRACK_KIND.contentMax, max: track.max };
    case "fixed": return { kind: BRIDGE_GRID_TRACK_KIND.fixed, size: track.size };
    case "flex": return { kind: BRIDGE_GRID_TRACK_KIND.flex };
    case "flexMax": return { kind: BRIDGE_GRID_TRACK_KIND.flexMax, max: track.max };
  }
}

function bridgeLayoutChild(entry: PendingAxisChild): BridgeLayoutChild {
  switch (entry.kind) {
    case BRIDGE_LAYOUT_CHILD_KIND.normal: return { kind: entry.kind, child: nodeForBridge(entry.view) };
    case BRIDGE_LAYOUT_CHILD_KIND.fixed: return { kind: entry.kind, size: entry.value ?? 0, child: nodeForBridge(entry.view) };
    case BRIDGE_LAYOUT_CHILD_KIND.flex: return { kind: entry.kind, child: nodeForBridge(entry.view) };
    case BRIDGE_LAYOUT_CHILD_KIND.flexMax: return { kind: entry.kind, maxRows: entry.value ?? 0, child: nodeForBridge(entry.view) };
    case BRIDGE_LAYOUT_CHILD_KIND.contentMax: return { kind: entry.kind, maxRows: entry.value ?? 0, child: nodeForBridge(entry.view) };
    default: throw new TypeError("unknown axis child kind");
  }
}

function axisTrackWord(entry: PendingAxisChild): number {
  switch (entry.kind) {
    case BRIDGE_LAYOUT_CHILD_KIND.normal: return 0;
    case BRIDGE_LAYOUT_CHILD_KIND.contentMax: return (2 | ((entry.value ?? 0) << 8)) >>> 0;
    case BRIDGE_LAYOUT_CHILD_KIND.fixed: return (3 | ((entry.value ?? 0) << 8)) >>> 0;
    case BRIDGE_LAYOUT_CHILD_KIND.flex: return (4 | (1 << 8)) >>> 0;
    case BRIDGE_LAYOUT_CHILD_KIND.flexMax: return (5 | ((entry.value ?? 0) << 8)) >>> 0;
    default: throw new TypeError("unknown axis child kind");
  }
}

function buildChildren(children: ChildBuilder): ChildrenBuilder {
  const builder = new ChildrenBuilder();
  if (typeof children === "function") children(builder);
  else builder.childrenOf(children);
  return builder;
}

function displayDiffRange(range: { readonly start: number; readonly count: number }): string {
  if (range.count === 0) return `${range.start},0`;
  const start = range.start + 1;
  return range.count === 1 ? `${start}` : `${start},${range.count}`;
}

function wrapCode(mode: WrapMode): number {
  if (mode === "wordThenGrapheme") return BRIDGE_WRAP_MODE.wordThenGrapheme;
  if (mode === "grapheme") return BRIDGE_WRAP_MODE.grapheme;
  return BRIDGE_WRAP_MODE.noWrap;
}

function horizontalAlignCode(align: HorizontalAlign): number {
  if (align === "start") return BRIDGE_HORIZONTAL_ALIGN.start;
  if (align === "center") return BRIDGE_HORIZONTAL_ALIGN.center;
  return BRIDGE_HORIZONTAL_ALIGN.end;
}

function verticalAlignCode(align: "top" | "center" | "bottom"): number {
  if (align === "top") return BRIDGE_VERTICAL_ALIGN.top;
  if (align === "center") return BRIDGE_VERTICAL_ALIGN.center;
  return BRIDGE_VERTICAL_ALIGN.bottom;
}

function validateU16(value: number, name: string): number {
  if (!Number.isInteger(value) || value < 0 || value > 65535) throw new RangeError(`${name} must be an integer from 0 to 65535`);
  return value;
}

function validatePositiveU16(value: number, name: string): number {
  if (!Number.isInteger(value) || value < 1 || value > 65535) throw new RangeError(`${name} must be an integer from 1 to 65535`);
  return value;
}

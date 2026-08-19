import type { NativeHandleId } from "../types.ts";
import {
  cloneDecoration,
  emptyDecoration,
  emptyStyle,
  mergeStyles,
  type BorderNode,
  type ColorNode,
  type DecorationNode,
  type DiffHunkNode,
  type GridCellNode,
  type GridRowNode,
  type GridTrackNode,
  type LayoutChild,
  type OverflowIndicatorNode,
  type ViewNode,
} from "../ir.ts";
import { insets, Insets } from "./geometry.ts";
import { StyleSpec } from "./style.ts";
import { TextSpan, type HorizontalAlign, type WrapMode } from "./text.ts";

type ChildBuilder = readonly View[] | ((builder: ChildrenBuilder) => void);

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
    } else {
      this.rows.push(build);
    }
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
  readonly children: LayoutChild[] = [];
  private layoutGap = 0;

  child(view: View): this { this.children.push({ kind: "normal", child: nodeForMaterialization(view) }); return this; }
  childrenOf(views: readonly View[]): this { for (const view of views) this.child(view); return this; }
  gap(value: number): this { this.layoutGap = validateU16(value, "gap"); return this; }
  fixed(size: number, view: View): this {
    this.children.push({ kind: "fixed", size: validateU16(size, "size"), child: nodeForMaterialization(view) });
    return this;
  }
  flex(view: View): this { this.children.push({ kind: "flex", child: nodeForMaterialization(view) }); return this; }
  flexMax(maxRows: number, view: View): this {
    validateU16(maxRows, "maxRows");
    this.children.push({ kind: "flexMax", maxRows, child: nodeForMaterialization(view) });
    return this;
  }
  contentMax(maxRows: number, view: View): this {
    validateU16(maxRows, "maxRows");
    this.children.push({ kind: "contentMax", maxRows, child: nodeForMaterialization(view) });
    return this;
  }

  gapValue(): number { return this.layoutGap; }
}

export class View {
  readonly kind = "view" as const;

  private constructor(private readonly node: ViewNode) {
    nodes.set(this, node);
  }

  static contentMax(maxRows: number, child: View): View {
    validateU16(maxRows, "maxRows");
    return new View({ type: "contentMax", child: child.node, maxRows });
  }

  static diff(hunks: readonly DiffHunkNode[]): View {
    return new View({ type: "diff", hunks: [...hunks] });
  }

  static text(value: string): View {
    if (typeof value !== "string") throw new TypeError("View.text requires a string");
    return new View({ type: "text", spans: [{ text: value }], wrap: "wordThenGrapheme", align: "start" });
  }

  static styledText(spans: readonly TextSpan[]): View {
    return new View({ type: "text", spans: spans.map((span) => ({ ...span.value })), wrap: "wordThenGrapheme", align: "start" });
  }

  static spacer(rows: number): View {
    validateU16(rows, "rows");
    return new View({ type: "spacer", rows });
  }

  static horizontal(children: ChildBuilder): View {
    const builder = buildChildren(children);
    return new View({ type: "row", children: builder.children, gap: builder.gapValue() });
  }

  static vertical(children: ChildBuilder): View {
    const builder = buildChildren(children);
    return new View({ type: "column", children: builder.children, gap: builder.gapValue() });
  }

  static hanging(prefix: View, continuation: View, body: View): View {
    return new View({ type: "hanging", prefix: prefix.node, continuation: continuation.node, body: body.node });
  }

  static grid(specification: readonly View[] | GridSpec | ((builder: GridBuilder) => void)): View {
    const builder = new GridBuilder();
    if (Array.isArray(specification)) {
      builder.columns(specification.map(() => ({ kind: "content" as const })));
      builder.row((row) => specification.forEach((view) => row.cell(view)));
    } else if (typeof specification === "function") {
      specification(builder);
    } else {
      const spec = specification as GridSpec;
      builder.columns(spec.columns ?? []);
      for (const row of spec.rows) builder.row(row);
      builder.columnGap(spec.columnGap ?? 0).rowGap(spec.rowGap ?? 0);
    }
    const rows: GridRowNode[] = builder.rows.map((row) => ({
      track: row.track ?? { kind: "content" },
      cells: row.cells.map((cell): GridCellNode => ({
        view: nodeForMaterialization(cell.view),
        columnSpan: validatePositiveU16(cell.columnSpan ?? 1, "columnSpan"),
        rowSpan: validatePositiveU16(cell.rowSpan ?? 1, "rowSpan"),
        horizontalAlign: cell.horizontalAlign ?? "start",
        verticalAlign: cell.verticalAlign ?? "top",
      })),
    }));
    return new View({
      type: "grid",
      columns: builder.columnsValue,
      rows,
      columnGap: builder.columnGapValue,
      rowGap: builder.rowGapValue,
    });
  }

  static component(handle: { readonly id: NativeHandleId; nativeComponentId?: () => number | undefined }): View {
    const nativeId = handle.nativeComponentId?.();
    return new View({ type: "component", handle: (nativeId ?? handle.id) as NativeHandleId });
  }

  bold(): View { return this.textAttribute("bold"); }
  dim(): View { return this.textAttribute("dim"); }
  italic(): View { return this.textAttribute("italic"); }
  underline(): View { return this.textAttribute("underline"); }
  reversed(): View { return this.textAttribute("reversed"); }
  strikethrough(): View { return this.textAttribute("strikethrough"); }

  textAttribute(name: string, enabled = true): View {
    return this.decorate({ style: { ...emptyStyle(), attributes: { [name]: enabled } } });
  }

  padding(value: number | Insets): View { return this.decorate({ padding: insets(value) }); }
  background(color: ColorNode): View { return this.decorate({ background: color }); }
  foreground(color: ColorNode): View { return this.decorate({ foreground: color }); }
  border(border: BorderNode): View { return this.decorate({ border }); }
  style(style: StyleSpec): View { return this.decorate({ style: mergeStyles(emptyStyle(), style.value) }); }
  styleState(key: string, value: string): View {
    if (key.length === 0 || value.length === 0) throw new RangeError("style state key and value cannot be empty");
    const current = this.node.type === "decorated" ? cloneDecoration(this.node.decoration) : emptyDecoration();
    const child = this.node.type === "decorated" ? this.node.child : this.node;
    return new View({
      type: "decorated",
      child,
      decoration: { ...current, styleStates: { ...current.styleStates, [key]: value } },
    });
  }
  container(): View { return new View({ type: "container", child: this.node }); }
  clampRows(maxRows: number, overflow: OverflowIndicator = { kind: "none" }): View {
    validateU16(maxRows, "maxRows");
    return new View({ type: "clamp", child: this.node, maxRows, overflow: overflowNode(overflow) });
  }
  fitWidth(): View { return this.decorate({ width: "fit" }); }
  fillWidth(): View { return this.decorate({ width: "fill" }); }
  fitHeight(): View { return this.decorate({ height: "fit" }); }
  fillHeight(): View { return this.decorate({ height: "fill" }); }
  minWidth(value: number): View { return this.decorate({ minWidth: validateU16(value, "minWidth") }); }
  maxWidth(value: number): View { return this.decorate({ maxWidth: validateU16(value, "maxWidth") }); }
  minHeight(value: number): View { return this.decorate({ minHeight: validateU16(value, "minHeight") }); }
  maxHeight(value: number): View { return this.decorate({ maxHeight: validateU16(value, "maxHeight") }); }
  wrap(mode: WrapMode): View { return this.mapText((text) => ({ ...text, wrap: mode })); }
  noWrap(): View { return this.wrap("noWrap"); }
  textAlign(align: HorizontalAlign): View { return this.mapText((text) => ({ ...text, align })); }

  private decorate(decoration: Partial<DecorationNode>): View {
    const current = this.node.type === "decorated" ? cloneDecoration(this.node.decoration) : emptyDecoration();
    const child = this.node.type === "decorated" ? this.node.child : this.node;
    const next: DecorationNode = {
      ...current,
      ...decoration,
      style: decoration.style === undefined ? current.style : mergeStyles(current.style, decoration.style),
    };
    return new View({ type: "decorated", child, decoration: next });
  }

  private mapText(map: (text: Extract<ViewNode, { type: "text" }>) => Extract<ViewNode, { type: "text" }>): View {
    if (this.node.type === "text") return new View(map(this.node));
    if (this.node.type === "decorated" && this.node.child.type === "text") {
      return new View({ ...this.node, child: map(this.node.child) });
    }
    return this;
  }
}

const nodes = new WeakMap<View, ViewNode>();

export function nodeForMaterialization(view: View): ViewNode {
  const node = nodes.get(view);
  if (node === undefined) {
    throw new TypeError("view is not a runtime semantic value");
  }
  return node;
}

export function textRowsForHarness(view: View): string[] {
  return rows(nodeForMaterialization(view));
}

function rows(node: ViewNode): string[] {
  switch (node.type) {
    case "text": return [node.spans.map((span) => span.text).join("")];
    case "diff": return node.hunks.flatMap((hunk) => [
      `@@ -${displayDiffRange(hunk.oldRange)} +${displayDiffRange(hunk.newRange)} @@`,
      ...hunk.lines.flatMap((line) => [
        `${line.kind === "addition" ? "+" : line.kind === "deletion" ? "-" : " "}${line.text}`,
        ...(line.termination === "unterminated" ? ["\\ No newline at end of file"] : []),
      ]),
    ]);
    case "spacer": return Array.from({ length: node.rows }, () => "");
    case "row": return [node.children.flatMap((child) => rows(child.child)).join("")];
    case "column": return node.children.flatMap((child) => rows(child.child));
    case "grid": return node.rows.flatMap((row) => row.cells.flatMap((cell) => rows(cell.view)));
    case "hanging": return rows(node.prefix).map((prefix, index) => `${prefix}${index === 0 ? rows(node.body)[0] ?? "" : rows(node.body)[index] ?? ""}`);
    case "container": case "clamp": return rows(node.child).slice(0, node.maxRows);
    case "contentMax": return rows(node.child).slice(0, node.maxRows);
    case "component": return [""];
    case "decorated": return rows(node.child);
  }
}

function overflowNode(overflow: OverflowIndicator): OverflowIndicatorNode {
  if (overflow.kind === "none") return overflow;
  if (overflow.kind === "ellipsis") return { kind: "ellipsis", style: overflow.style.value };
  return { kind: "footer", prefix: overflow.prefix, style: overflow.style.value };
}

function buildChildren(children: ChildBuilder): ChildrenBuilder {
  const builder = new ChildrenBuilder();
  if (typeof children === "function") {
    children(builder);
  } else {
    builder.childrenOf(children);
  }
  return builder;
}

function displayDiffRange(range: { readonly start: number; readonly count: number }): string {
  if (range.count === 0) return `${range.start},0`;
  const start = range.start + 1;
  return range.count === 1 ? `${start}` : `${start},${range.count}`;
}

function validateU16(value: number, name: string): number {
  if (!Number.isInteger(value) || value < 0 || value > 65535) throw new RangeError(`${name} must be an integer from 0 to 65535`);
  return value;
}

function validatePositiveU16(value: number, name: string): number {
  if (!Number.isInteger(value) || value < 1 || value > 65535) throw new RangeError(`${name} must be an integer from 1 to 65535`);
  return value;
}

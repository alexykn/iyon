import type { NativeHandleId } from "../types.ts";
import {
  cloneDecoration,
  emptyDecoration,
  emptyStyle,
  mergeStyles,
  type BorderNode,
  type ColorNode,
  type DecorationNode,
  type ViewNode,
} from "../ir.ts";
import { insets, Insets } from "./geometry.ts";
import { StyleSpec } from "./style.ts";
import { TextSpan, type HorizontalAlign, type WrapMode } from "./text.ts";

type ChildBuilder = readonly View[] | ((builder: ChildrenBuilder) => void);

export class ChildrenBuilder {
  readonly children: View[] = [];

  child(view: View): this { this.children.push(view); return this; }
  childrenOf(views: readonly View[]): this { this.children.push(...views); return this; }
  gap(_value: number): this { return this; }
  fixed(_size: number, view: View): this { this.children.push(view); return this; }
  flex(view: View): this { this.children.push(view); return this; }
}

export class View {
  readonly kind = "view" as const;

  private constructor(private readonly node: ViewNode) {
    nodes.set(this, node);
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
    return new View({ type: "row", children: builder.children.map((child) => child.node), gap: 0 });
  }

  static vertical(children: ChildBuilder): View {
    const builder = buildChildren(children);
    return new View({ type: "column", children: builder.children.map((child) => child.node), gap: 0 });
  }

  static hanging(prefix: View, continuation: View, body: View): View {
    return new View({ type: "hanging", prefix: prefix.node, continuation: continuation.node, body: body.node });
  }

  static grid(children: readonly View[]): View {
    return new View({ type: "grid", children: children.map((child) => child.node) });
  }

  static component(handle: { readonly id: NativeHandleId }): View {
    return new View({ type: "component", handle: handle.id });
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
  container(): View { return new View({ type: "container", child: this.node }); }
  clampRows(maxRows: number): View { validateU16(maxRows, "maxRows"); return new View({ type: "clamp", child: this.node, maxRows }); }
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
    case "spacer": return Array.from({ length: node.rows }, () => "");
    case "row": return [node.children.flatMap(rows).join("")];
    case "column": case "grid": return node.children.flatMap(rows);
    case "hanging": return rows(node.prefix).map((prefix, index) => `${prefix}${index === 0 ? rows(node.body)[0] ?? "" : rows(node.body)[index] ?? ""}`);
    case "container": case "clamp": return rows(node.child).slice(0, node.maxRows);
    case "component": return [""];
    case "decorated": return rows(node.child);
  }
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

function validateU16(value: number, name: string): number {
  if (!Number.isInteger(value) || value < 0 || value > 65535) throw new RangeError(`${name} must be an integer from 0 to 65535`);
  return value;
}

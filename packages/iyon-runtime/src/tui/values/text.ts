import type { StyleSpec } from "./style.ts";
import type { StyleNode, TextSpanNode } from "../ir.ts";

export type WrapMode = "wordThenGrapheme" | "grapheme" | "noWrap";
export type HorizontalAlign = "start" | "center" | "end";

export class TextSpan {
  readonly kind = "text-span" as const;

  constructor(readonly value: TextSpanNode) {}

  static plain(text: string): TextSpan {
    return new TextSpan({ text });
  }

  static styled(text: string, style: StyleSpec): TextSpan {
    return new TextSpan({ text, style: style.value });
  }
}

export function textStyle(value: StyleSpec | undefined): StyleNode | undefined {
  return value?.value;
}

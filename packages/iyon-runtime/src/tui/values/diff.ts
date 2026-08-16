import { View } from "./view.ts";

export type DiffLineKind = "context" | "addition" | "deletion";
export type DiffLineTermination = "lf" | "crlf" | "none";

export class DiffRange {
  readonly kind = "diff-range" as const;
  constructor(readonly start: number, readonly end: number) { if (start < 0 || end < start) throw new RangeError("invalid diff range"); }
}

export class DiffLine {
  readonly kind = "diff-line" as const;
  constructor(readonly lineKind: DiffLineKind, readonly text: string, readonly termination: DiffLineTermination = "lf") {}
}

export class DiffHunk {
  readonly kind = "diff-hunk" as const;
  constructor(readonly oldRange: DiffRange, readonly newRange: DiffRange, readonly lines: readonly DiffLine[] = []) {}
  render(): View { return View.vertical(this.lines.map((line) => View.text(`${prefix(line.lineKind)}${line.text}`))); }
}

export class DiffRenderer {
  render(hunk: DiffHunk): View { return hunk.render(); }
}

function prefix(kind: DiffLineKind): string { return kind === "addition" ? "+" : kind === "deletion" ? "-" : " "; }

import { View } from "./view.ts";
import type { DiffHunkNode } from "../ir.ts";

export type DiffLineKind = "context" | "addition" | "deletion";
export type DiffLineTermination = "lf" | "crlf" | "none";

export class DiffRange {
  readonly kind = "diff-range" as const;

  constructor(readonly start: number, readonly end: number) {
    if (!Number.isSafeInteger(start) || !Number.isSafeInteger(end) || start < 0 || end < start) {
      throw new RangeError("invalid diff range");
    }
  }
}

export class DiffLine {
  readonly kind = "diff-line" as const;

  constructor(
    readonly lineKind: DiffLineKind,
    readonly text: string,
    readonly termination: DiffLineTermination = "lf",
  ) {}
}

export class DiffHunk {
  readonly kind = "diff-hunk" as const;

  constructor(
    readonly oldRange: DiffRange,
    readonly newRange: DiffRange,
    readonly lines: readonly DiffLine[] = [],
  ) {
    this.validate();
  }

  validate(): void {
    let oldConsumed = 0;
    let newConsumed = 0;
    for (const line of this.lines) {
      if (line.lineKind !== "addition") oldConsumed += 1;
      if (line.lineKind !== "deletion") newConsumed += 1;
    }
    const expectedOld = this.oldRange.end - this.oldRange.start;
    const expectedNew = this.newRange.end - this.newRange.start;
    if (oldConsumed !== expectedOld || newConsumed !== expectedNew) {
      throw new RangeError(`diff hunk consumed old ${oldConsumed}/${expectedOld} and new ${newConsumed}/${expectedNew} lines`);
    }
  }

  render(): View {
    return new DiffRenderer().render(this);
  }
}

/** Semantic diff renderer. Rust lowers the diff node into the themed View. */
export class DiffRenderer {
  render(hunks: DiffHunk | readonly DiffHunk[]): View {
    const values = Array.isArray(hunks) ? hunks : [hunks];
    return View.diff(values.map(toNode));
  }

  renderHunk(hunk: DiffHunk): View {
    return this.render(hunk);
  }
}

function toNode(hunk: DiffHunk): DiffHunkNode {
  let oldLine = hunk.oldRange.start + 1;
  let newLine = hunk.newRange.start + 1;
  const lines = hunk.lines.map((line) => {
    const node = {
      kind: line.lineKind,
      text: line.text,
      termination: line.termination === "none" ? "unterminated" : "terminated",
      ...(line.lineKind === "context" ? { oldLine, newLine } : {}),
      ...(line.lineKind === "addition" ? { newLine } : {}),
      ...(line.lineKind === "deletion" ? { oldLine } : {}),
    } as const;
    if (line.lineKind !== "addition") oldLine += 1;
    if (line.lineKind !== "deletion") newLine += 1;
    return node;
  });
  return {
    oldRange: { start: hunk.oldRange.start, count: hunk.oldRange.end - hunk.oldRange.start },
    newRange: { start: hunk.newRange.start, count: hunk.newRange.end - hunk.newRange.start },
    lines,
  };
}

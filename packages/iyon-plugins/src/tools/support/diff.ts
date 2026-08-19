export type DiffLineKind = "context" | "addition" | "deletion";
export type DiffLineTermination = "lf" | "crlf" | "none";
export interface DiffLine { readonly kind: DiffLineKind; readonly text: string; readonly termination?: DiffLineTermination; }
export interface DiffHunk { readonly oldStart: number; readonly oldCount: number; readonly newStart: number; readonly newCount: number; readonly lines: readonly DiffLine[]; }

export function unifiedDiff(path: string, before: string, after: string): string {
  const oldLines = splitLines(before);
  const newLines = splitLines(after);
  const rows = diffLines(oldLines, newLines);
  if (rows.every((row) => row.kind === "context")) return "";
  const oldCount = oldLines.length;
  const newCount = newLines.length;
  const body = rows.map((row) => `${row.kind === "addition" ? "+" : row.kind === "deletion" ? "-" : " "}${row.text}`).join("\n");
  return `--- a/${path}\n+++ b/${path}\n@@ -1,${oldCount} +1,${newCount} @@\n${body}\n`;
}

export function normalizeDiff(diff: string): string { return diff.replaceAll("\r\n", "\n").replaceAll("\r", "\n"); }

export function parseUnifiedDiff(diff: string): DiffHunk[] {
  const lines = normalizeDiff(diff).split("\n");
  const hunks: DiffHunk[] = [];
  let current: DiffHunk | undefined;
  for (const line of lines) {
    if (line.startsWith("@@ ")) {
      const match = line.match(/^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/);
      if (!match) throw new Error("malformed unified diff hunk");
      current = { oldStart: Number(match[1]), oldCount: Number(match[2] ?? 1), newStart: Number(match[3]), newCount: Number(match[4] ?? 1), lines: [] };
      hunks.push(current);
      continue;
    }
    if (line === "\\ No newline at end of file") {
      if (!current || current.lines.length === 0) throw new Error("orphaned no-newline marker");
      const lines = [...current.lines];
      lines[lines.length - 1] = { ...lines[lines.length - 1]!, termination: "none" };
      current = { ...current, lines };
      hunks[hunks.length - 1] = current;
      continue;
    }
    if (!current || line.startsWith("--- ") || line.startsWith("+++ ") || line === "") continue;
    const prefix = line[0];
    if (prefix !== " " && prefix !== "+" && prefix !== "-") throw new Error("malformed unified diff line");
    current = { ...current, lines: [...current.lines, { kind: prefix === "+" ? "addition" : prefix === "-" ? "deletion" : "context", text: line.slice(1) }] };
    hunks[hunks.length - 1] = current;
  }
  if (hunks.length === 0) throw new Error("unified diff contains no hunks");
  return hunks;
}

function splitLines(value: string): string[] { return value.replaceAll("\r\n", "\n").replaceAll("\r", "\n").split("\n"); }

function diffLines(oldLines: readonly string[], newLines: readonly string[]): DiffLine[] {
  const table = Array.from({ length: oldLines.length + 1 }, () => Array<number>(newLines.length + 1).fill(0));
  for (let oldIndex = oldLines.length - 1; oldIndex >= 0; oldIndex--) for (let newIndex = newLines.length - 1; newIndex >= 0; newIndex--) table[oldIndex]![newIndex] = oldLines[oldIndex] === newLines[newIndex] ? table[oldIndex + 1]![newIndex + 1]! + 1 : Math.max(table[oldIndex + 1]![newIndex]!, table[oldIndex]![newIndex + 1]!);
  const result: DiffLine[] = [];
  let oldIndex = 0;
  let newIndex = 0;
  while (oldIndex < oldLines.length || newIndex < newLines.length) {
    if (oldIndex < oldLines.length && newIndex < newLines.length && oldLines[oldIndex] === newLines[newIndex]) { result.push({ kind: "context", text: oldLines[oldIndex]! }); oldIndex++; newIndex++; continue; }
    if (newIndex < newLines.length && (oldIndex === oldLines.length || table[oldIndex]![newIndex + 1]! >= table[oldIndex + 1]![newIndex]!)) { result.push({ kind: "addition", text: newLines[newIndex]! }); newIndex++; continue; }
    result.push({ kind: "deletion", text: oldLines[oldIndex]! }); oldIndex++;
  }
  return result;
}

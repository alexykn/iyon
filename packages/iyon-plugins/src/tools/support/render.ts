import { parseUnifiedDiff } from "./diff.ts";
import type { DiffHunk } from "./diff.ts";
import { View } from "iyon:tui";

export function renderDiff(details: unknown): View | undefined {
  const diff = typeof details === "object" && details !== null && typeof (details as { diff?: unknown }).diff === "string" ? (details as { diff: string }).diff : undefined;
  if (!diff) return undefined;
  try { return renderHunks(parseUnifiedDiff(diff)); } catch { return View.vertical(diff.split("\n").map((line) => View.text(line).fillWidth())).fillWidth() as unknown as View; }
}

export function renderHunks(hunks: readonly DiffHunk[]): View {
  return View.vertical(hunks.flatMap((hunk) => hunk.lines.map((line) => View.text(`${line.kind === "addition" ? "+" : line.kind === "deletion" ? "-" : " "}${line.text}`).fillWidth()))).fillWidth() as unknown as View;
}

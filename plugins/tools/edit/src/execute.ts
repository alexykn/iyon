import { defineTool, type ToolContext } from "@iyon/sdk";
import { unifiedDiff, resolveWorkspacePath, writeWorkspaceText, withMutation } from "@iyon/plugins";
import { readFile } from "node:fs/promises";
import { renderEditCall, renderEditResult } from "./render.ts";

interface TextEdit { oldText: string; newText: string }
interface EditInput { path: string; edits: TextEdit[] }
type LineEnding = "lf" | "crlf" | "cr";

export const editTool = defineTool({
  name: "edit",
  description: "Edit a single file using exact text replacement. Every edits[].oldText must match a unique, non-overlapping region of the original file. If two changes affect the same block or nearby lines, merge them into one edit instead of emitting overlapping edits. Do not include large unchanged regions just to connect distant changes.",
  inputSchema: {
    type: "object",
    properties: { path: { type: "string", description: "Path to the file to edit (relative or absolute)" }, edits: { type: "array", description: "One or more targeted replacements. Each edit is matched against the original file, not incrementally. Do not include overlapping or nested edits.", items: { type: "object", properties: { oldText: { type: "string", description: "Exact text for one targeted replacement. It must be unique in the original file." }, newText: { type: "string", description: "Replacement text for this targeted edit." } }, required: ["oldText", "newText"], additionalProperties: false } } },
    required: ["path", "edits"],
    additionalProperties: false,
  },
  execution: { executionMode: "sequential", approval: "neverAsk", promptSnippet: "Make precise file edits with exact text replacement, including multiple disjoint edits in one call" },
  execute: async (context: ToolContext, raw: Record<string, unknown>) => {
    const input = parseInput(raw);
    validateInput(input);
    checkCancelled(context);
    const resolved = await resolveWorkspacePath(context.workspace, input.path, "write");
    return await withMutation(resolved, async () => {
      checkCancelled(context);
      const rawText = await readOriginal(context, input.path, resolved);
      checkCancelled(context);
      const { bom, content } = stripBom(rawText);
      const lineEnding = detectLineEnding(content);
      const base = normalizeToLf(content);
      const edits = input.edits.map((edit) => ({ oldText: normalizeToLf(edit.oldText), newText: normalizeToLf(edit.newText) }));
      const ranges = edits.map((edit) => findUniqueRange(base, edit.oldText, input.path));
      validateNonOverlapping(ranges, input.path);
      const updated = applyReplacements(base, edits, ranges);
      await writeWorkspaceText(context.workspace, input.path, `${bom}${restoreLineEndings(updated, lineEnding)}`);
      checkCancelled(context);
      return { content: [{ type: "text", text: `Successfully replaced ${input.edits.length} block(s) in ${input.path}.` }], details: { diff: unifiedDiff(input.path, base, updated), firstChangedLine: firstChangedLine(base, ranges) }, isError: false };
    });
  },
  renderCall: renderEditCall,
  renderResult: renderEditResult,
});

function parseInput(raw: Record<string, unknown>): EditInput {
  const input = { ...raw };
  if (typeof input.edits === "string") { try { const parsed = JSON.parse(input.edits); if (Array.isArray(parsed)) input.edits = parsed; } catch { /* schema validation below reports the malformed payload */ } }
  if (input.oldText !== undefined && input.newText !== undefined) { const edits = Array.isArray(input.edits) ? [...input.edits] : []; edits.push({ oldText: input.oldText, newText: input.newText }); input.edits = edits; }
  if (!Array.isArray(input.edits)) throw new Error("invalid edit input");
  return { path: String(input.path ?? ""), edits: input.edits.map((edit) => ({ oldText: String((edit as Record<string, unknown>).oldText ?? ""), newText: String((edit as Record<string, unknown>).newText ?? "") })) };
}

function validateInput(input: EditInput): void { if (!input.path.trim()) throw new Error("edit path must not be empty"); if (input.edits.length === 0) throw new Error("Edit tool input is invalid. edits must contain at least one replacement."); if (input.edits.some((edit) => !edit.oldText)) throw new Error("edit oldText must not be empty"); }

async function readOriginal(context: ToolContext, inputPath: string, resolvedPath: string): Promise<string> {
  if (context.workspace.readText) return await context.workspace.readText(inputPath);
  const bytes = await readFile(resolvedPath);
  return new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes);
}

function findUniqueRange(content: string, oldText: string, path: string): { start: number; end: number } { const matches = [...content.matchAll(new RegExp(escapeRegex(oldText), "g"))].map((match) => match.index ?? 0); if (matches.length === 0) throw new Error(`oldText not found in ${path}`); if (matches.length > 1) throw new Error(`oldText must be unique in ${path}`); return { start: matches[0]!, end: matches[0]! + oldText.length }; }
function validateNonOverlapping(ranges: readonly { start: number; end: number }[], path: string): void { const sorted = [...ranges].sort((left, right) => left.start - right.start); for (const [left, right] of sorted.slice(0, -1).map((left, index) => [left, sorted[index + 1]!] as const)) if (left.end > right.start) throw new Error(`edit replacements overlap in ${path}`); }
function applyReplacements(content: string, edits: readonly TextEdit[], ranges: readonly { start: number; end: number }[]): string { return ranges.map((range, index) => ({ range, edit: edits[index]! })).sort((left, right) => right.range.start - left.range.start).reduce((value, item) => value.slice(0, item.range.start) + item.edit.newText + value.slice(item.range.end), content); }
function stripBom(value: string): { bom: string; content: string } { return value.startsWith("\uFEFF") ? { bom: "\uFEFF", content: value.slice(1) } : { bom: "", content: value }; }
function detectLineEnding(value: string): LineEnding { return value.includes("\r\n") ? "crlf" : value.includes("\r") ? "cr" : "lf"; }
function normalizeToLf(value: string): string { return value.replaceAll("\r\n", "\n").replaceAll("\r", "\n"); }
function restoreLineEndings(value: string, ending: LineEnding): string { return ending === "crlf" ? value.replaceAll("\n", "\r\n") : ending === "cr" ? value.replaceAll("\n", "\r") : value; }
function firstChangedLine(content: string, ranges: readonly { start: number }[]): number | undefined { const start = Math.min(...ranges.map((range) => range.start)); return Number.isFinite(start) ? content.slice(0, start).split("\n").length : undefined; }
function escapeRegex(value: string): string { return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"); }
function checkCancelled(context: ToolContext): void { if (context.signal.aborted) throw new Error("edit tool cancelled"); }

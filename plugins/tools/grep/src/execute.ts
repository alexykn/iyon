import { defineTool, type ToolContext } from "@iyon/sdk";
import { DEFAULT_MODEL_MAX_BYTES, GREP_MAX_LINE_CHARS, findProgram, runCapture, truncateHead, truncateLine, resolveWorkspacePath } from "@iyon/plugins";
import { readdir, readFile } from "node:fs/promises";
import { join, relative } from "node:path";
import { renderGrepCall, renderGrepResult } from "./render.ts";

const DEFAULT_LIMIT = 100;

export const grepTool = defineTool({
  name: "grep",
  description: `Search file contents for a pattern. Prefers ripgrep and falls back to grep. Returns matching lines with file paths and line numbers. Output is truncated to ${DEFAULT_LIMIT} matches or ${DEFAULT_MODEL_MAX_BYTES / 1024}KB (whichever is hit first). Long lines are truncated to ${GREP_MAX_LINE_CHARS} chars.`,
  inputSchema: {
    type: "object",
    properties: { pattern: { type: "string", description: "Search pattern (regex or literal string)" }, path: { type: "string", description: "Directory or file to search (default: current directory)" }, glob: { type: "string", description: "Filter files by glob pattern, e.g. '*.ts' or '**/*.spec.ts'" }, ignoreCase: { type: "boolean", description: "Case-insensitive search (default: false)" }, literal: { type: "boolean", description: "Treat pattern as literal string instead of regex (default: false)" }, context: { type: "number", description: "Number of lines to show before and after each match (default: 0)" }, limit: { type: "number", description: "Maximum number of output lines to return (default: 100)" } },
    required: ["pattern"],
    additionalProperties: false,
  },
  execution: { executionMode: "parallel", approval: "neverAsk", promptSnippet: "Search file contents for patterns" },
  execute: async (context: ToolContext, args: { pattern: string; path?: string; glob?: string; ignoreCase?: boolean; literal?: boolean; context?: number; limit?: number }) => {
    if (!args?.pattern?.trim()) throw new Error("grep pattern must not be empty");
    const searchPath = await resolveWorkspacePath(context.workspace, args.path ?? ".", "search");
    const limit = Math.max(1, args.limit ?? DEFAULT_LIMIT);
    const found = await search(context, searchPath, args, limit);
    if (found.lines.length === 0) return { content: [{ type: "text", text: "No matches found" }], details: {}, isError: false };
    const truncated = truncateHead(found.lines.join("\n"), { maxLines: Number.MAX_SAFE_INTEGER, maxBytes: DEFAULT_MODEL_MAX_BYTES });
    const details: Record<string, unknown> = {};
    const notices: string[] = [];
    if (found.limitReached) { details.matchLimitReached = limit; notices.push(`${limit} matches limit reached`); }
    if (found.linesTruncated) { details.linesTruncated = true; notices.push("some lines truncated"); }
    if (truncated.report.truncated) { details.truncation = truncated.report; notices.push(`${DEFAULT_MODEL_MAX_BYTES / 1024}KB limit reached`); }
    const text = notices.length ? `${truncated.text}\n\n[Truncated: ${notices.join(", ")}]` : truncated.text;
    return { content: [{ type: "text", text }], details, isError: false };
  },
  renderCall: renderGrepCall,
  renderResult: renderGrepResult,
});

interface SearchArgs { pattern: string; path?: string; glob?: string; ignoreCase?: boolean; literal?: boolean; context?: number; limit?: number }
interface SearchResult { lines: string[]; limitReached: boolean; linesTruncated: boolean }

async function search(context: ToolContext, root: string, args: SearchArgs, limit: number): Promise<SearchResult> {
  const program = findProgram("rg") ?? findProgram("grep");
  if (program) {
    const isRg = program.endsWith("/rg") || program === "rg";
    const commandArgs = isRg ? ["--line-number", "--color=never", "--hidden", ...(args.ignoreCase ? ["--ignore-case"] : []), ...(args.literal ? ["--fixed-strings"] : []), ...(args.context ? ["--context", String(args.context)] : []), ...(args.glob ? ["--glob", args.glob] : []), "--", args.pattern, root] : ["-R", "-n", "-I", ...(args.ignoreCase ? ["-i"] : []), ...(args.literal ? ["-F"] : []), ...(args.context ? ["-C", String(args.context)] : []), ...(args.glob ? [`--include=${args.glob}`] : []), "--", args.pattern, root];
    const output = await runCapture({ program, args: commandArgs, cwd: context.cwd }, context.signal);
    if (output.exitCode === 1 && output.stdout.length === 0) return { lines: [], limitReached: false, linesTruncated: false };
    if (output.exitCode !== null && output.exitCode !== 0 && output.stdout.length === 0) throw new Error(`${isRg ? "rg" : "grep"} failed: ${new TextDecoder().decode(output.stderr).trim() || "unknown error"}`);
    return parseLines(new TextDecoder().decode(output.stdout), limit);
  }
  return searchNode(root, root, args, limit);
}

function parseLines(raw: string, limit: number): SearchResult { const lines: string[] = []; let linesTruncated = false; for (const line of raw.split("\n").filter(Boolean)) { if (lines.length >= limit) break; const result = truncateLine(line, GREP_MAX_LINE_CHARS); linesTruncated ||= result.truncated; lines.push(result.text); } return { lines, limitReached: raw.split("\n").filter(Boolean).length > limit, linesTruncated }; }

async function searchNode(root: string, current: string, args: SearchArgs, limit: number): Promise<SearchResult> {
  const output: string[] = [];
  const entries = await readdir(current, { withFileTypes: true });
  for (const entry of entries) {
    if (entry.name === ".git" || entry.name === "node_modules") continue;
    const path = join(current, entry.name);
    if (entry.isDirectory()) { const nested = await searchNode(root, path, args, limit - output.length); output.push(...nested.lines); if (nested.limitReached || output.length >= limit) return { lines: output.slice(0, limit), limitReached: true, linesTruncated: nested.linesTruncated }; continue; }
    if (args.glob && !matchesGlob(relative(root, path).replaceAll("\\", "/"), args.glob)) continue;
    let text: string;
    try { text = await readFile(path, "utf8"); } catch { continue; }
    const source = args.ignoreCase ? text.toLowerCase() : text;
    const needle = args.ignoreCase ? args.pattern.toLowerCase() : args.pattern;
    const expression = args.literal ? undefined : new RegExp(args.pattern, args.ignoreCase ? "i" : "");
    const fileLines = text.split("\n");
    for (let index = 0; index < fileLines.length; index++) {
      const line = fileLines[index]!;
      if (!(expression ? expression.test(line) : source.includes(needle))) continue;
      const result = truncateLine(`${relative(root, path).replaceAll("\\", "/")}:${index + 1}:${line}`, GREP_MAX_LINE_CHARS);
      output.push(result.text);
      if (output.length >= limit) return { lines: output, limitReached: true, linesTruncated: result.truncated };
    }
  }
  return { lines: output, limitReached: false, linesTruncated: false };
}

function matchesGlob(value: string, pattern: string): boolean { const expression = pattern.split("**").map((part) => part.replace(/[.+^${}()|[\]\\]/g, "\\$&")).join(".*").replaceAll("\\*", "[^/]*"); return new RegExp(`^${expression}$`).test(value); }

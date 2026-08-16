import { defineTool, type ToolContext } from "@iyon/sdk";
import { DEFAULT_MODEL_MAX_BYTES, findProgram, runCapture, truncateHead, resolveWorkspacePath } from "@iyon/plugins";
import { readdir, stat } from "node:fs/promises";
import { join, relative } from "node:path";
import { renderFindCall, renderFindResult } from "./render.ts";

const DEFAULT_LIMIT = 1_000;

export const findTool = defineTool({
  name: "find",
  description: `Search for files by glob pattern. Prefers fd and falls back to find. Output is truncated to ${DEFAULT_LIMIT} results or ${DEFAULT_MODEL_MAX_BYTES / 1024}KB (whichever is hit first).`,
  inputSchema: {
    type: "object",
    properties: { pattern: { type: "string", description: "Glob pattern to match files, e.g. '*.ts', '**/*.json', or 'src/**/*.spec.ts'" }, path: { type: "string", description: "Directory to search in (default: current directory)" }, limit: { type: "number", description: "Maximum number of results (default: 1000)" } },
    required: ["pattern"],
    additionalProperties: false,
  },
  execution: { executionMode: "parallel", approval: "neverAsk", promptSnippet: "Find files by glob pattern" },
  execute: async (context: ToolContext, args: { pattern: string; path?: string; limit?: number }) => {
    if (!args?.pattern?.trim()) throw new Error("find pattern must not be empty");
    const root = await resolveWorkspacePath(context.workspace, args.path ?? ".", "search");
    const limit = Math.max(1, args.limit ?? DEFAULT_LIMIT);
    const paths = await findPaths(root, args.pattern, limit, context);
    if (paths.length === 0) return { content: [{ type: "text", text: "No files found matching pattern" }], details: {}, isError: false };
    const truncated = truncateHead(paths.join("\n"), { maxLines: Number.MAX_SAFE_INTEGER, maxBytes: DEFAULT_MODEL_MAX_BYTES });
    const details: Record<string, unknown> = {};
    const notices: string[] = [];
    if (paths.length >= limit) { details.resultLimitReached = limit; notices.push(`${limit} results limit reached. Use limit=${limit * 2} for more, or refine pattern`); }
    if (truncated.report.truncated) { details.truncation = truncated.report; notices.push(`${DEFAULT_MODEL_MAX_BYTES / 1024}KB limit reached`); }
    const text = notices.length ? `${truncated.text}\n\n[${notices.join(". ")}]` : truncated.text;
    return { content: [{ type: "text", text }], details, isError: false };
  },
  renderCall: renderFindCall,
  renderResult: renderFindResult,
});

async function findPaths(root: string, pattern: string, limit: number, context: ToolContext): Promise<string[]> {
  const fd = findProgram("fd");
  if (fd) {
    const args = ["--glob", "--color=never", "--hidden", "--no-require-git", "--max-results", String(limit), ...(pattern.includes("/") ? ["--full-path"] : []), "--", pattern, root];
    const output = await runCapture({ program: fd, args, cwd: context.cwd }, context.signal);
    if (output.exitCode !== null && output.exitCode !== 0 && output.stdout.length === 0) throw new Error(`fd failed: ${new TextDecoder().decode(output.stderr).trim() || "unknown error"}`);
    return decodePaths(output.stdout, root, Number.MAX_SAFE_INTEGER).filter((path) => !path.startsWith("node_modules/") && !path.includes("/node_modules/")).slice(0, limit);
  }
  const systemFind = findProgram("find");
  if (systemFind) {
    const output = await runCapture({ program: systemFind, args: [root, "-name", ".git", "-prune", "-o", "-name", "node_modules", "-prune", "-o", "-type", "f", "-print"], cwd: context.cwd }, context.signal);
    if (output.exitCode !== null && output.exitCode !== 0 && output.stdout.length === 0) throw new Error(`find failed: ${new TextDecoder().decode(output.stderr).trim() || "unknown error"}`);
    return decodePaths(output.stdout, root, Number.MAX_SAFE_INTEGER).filter((path) => !path.startsWith("node_modules/") && !path.includes("/node_modules/") && matchesGlob(path, pattern)).slice(0, limit);
  }
  return walk(root, root, pattern, limit);
}

function decodePaths(bytes: Uint8Array, root: string, limit: number): string[] { return new TextDecoder().decode(bytes).split("\n").map((path) => path.trim()).filter(Boolean).slice(0, limit).map((path) => relative(root, path).replaceAll("\\", "/") || "."); }

async function walk(root: string, current: string, pattern: string, limit: number): Promise<string[]> {
  const output: string[] = [];
  for (const entry of await readdir(current, { withFileTypes: true })) {
    if (entry.name === ".git" || entry.name === "node_modules") continue;
    const absolute = join(current, entry.name);
    if (entry.isDirectory()) output.push(...await walk(root, absolute, pattern, limit - output.length));
    else if (matchesGlob(relative(root, absolute).replaceAll("\\", "/"), pattern)) output.push(relative(root, absolute).replaceAll("\\", "/"));
    if (output.length >= limit) break;
  }
  return output.slice(0, limit);
}

function matchesGlob(value: string, pattern: string): boolean { const expression = pattern.split("**").map(escapeRegex).join(".*").replaceAll("\\*", "[^/]*").replaceAll("\\?", "[^/]"); return new RegExp(`^${expression}$`).test(value) || new RegExp(`(^|/)${expression}$`).test(value); }
function escapeRegex(value: string): string { return value.replace(/[.+^${}()|[\]\\]/g, "\\$&"); }

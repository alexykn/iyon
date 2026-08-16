import { defineTool, type ToolContext } from "@iyon/sdk";
import { DEFAULT_MODEL_MAX_BYTES, truncateHead, resolveWorkspacePath } from "@iyon/plugins";
import { readdir, stat } from "node:fs/promises";
import { renderLsCall, renderLsResult } from "./render.ts";

const DEFAULT_LIMIT = 500;

export const lsTool = defineTool({
  name: "ls",
  description: `List directory contents. Returns entries sorted alphabetically, with '/' suffix for directories. Includes dotfiles. Output is truncated to ${DEFAULT_LIMIT} entries or ${DEFAULT_MODEL_MAX_BYTES / 1024}KB (whichever is hit first).`,
  inputSchema: {
    type: "object",
    properties: { path: { type: "string", description: "Directory to list (default: current directory)" }, limit: { type: "number", description: "Maximum number of entries to return (default: 500)" } },
    additionalProperties: false,
  },
  execution: { executionMode: "parallel", approval: "neverAsk", promptSnippet: "List directory contents" },
  execute: async (context: ToolContext, args: { path?: string; limit?: number }) => {
    const directory = await resolveWorkspacePath(context.workspace, args?.path ?? ".", "search");
    const limit = Math.max(1, args?.limit ?? DEFAULT_LIMIT);
    const entries = await readdir(directory, { withFileTypes: true });
    entries.sort((left, right) => left.name.toLowerCase().localeCompare(right.name.toLowerCase()));
    const selected = entries.slice(0, limit).map((entry) => `${entry.name}${entry.isDirectory() ? "/" : ""}`);
    if (selected.length === 0) return { content: [{ type: "text", text: "(empty directory)" }], details: {}, isError: false };
    const truncated = truncateHead(selected.join("\n"), { maxLines: Number.MAX_SAFE_INTEGER, maxBytes: DEFAULT_MODEL_MAX_BYTES });
    const details: Record<string, unknown> = {};
    const notices: string[] = [];
    if (entries.length > selected.length) { details.entryLimitReached = limit; notices.push(`${limit} entries limit reached. Use limit=${limit * 2} for more`); }
    if (truncated.report.truncated) { details.truncation = truncated.report; notices.push(`${DEFAULT_MODEL_MAX_BYTES / 1024}KB limit reached`); }
    const text = notices.length ? `${truncated.text}\n\n[${notices.join(". ")}]` : truncated.text;
    return { content: [{ type: "text", text }], details, isError: false };
  },
  renderCall: renderLsCall,
  renderResult: renderLsResult,
});

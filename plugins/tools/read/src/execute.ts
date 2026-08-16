import { defineTool, type ToolContext } from "@iyon/sdk";
import { readWorkspaceText } from "@iyon/plugins";
import { renderReadCall, renderReadResult } from "./render.ts";

export const readTool = defineTool({
  name: "read",
  description: "Read a UTF-8 text file from the workspace.",
  inputSchema: {
    type: "object",
    properties: { path: { type: "string", description: "Path to the file to read, relative to the workspace root." } },
    required: ["path"],
    additionalProperties: false,
  },
  execution: { executionMode: "parallel", approval: "neverAsk", promptSnippet: "read: Read a UTF-8 text file from the workspace." },
  execute: async (context: ToolContext, args: { path: string }) => {
    if (!args || typeof args.path !== "string") throw new Error("read tool requires string field: path");
    if (!args.path.trim()) throw new Error("read tool path must not be empty");
    if (context.signal.aborted) throw new Error("read tool cancelled");
    const text = await readWorkspaceText(context.workspace, args.path);
    if (context.signal.aborted) throw new Error("read tool cancelled");
    return { content: [{ type: "text", text }], details: { path: args.path }, isError: false };
  },
  renderCall: renderReadCall,
  renderResult: renderReadResult,
});

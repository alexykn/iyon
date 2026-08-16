import { defineTool, type ToolContext } from "@iyon/sdk";
import { unifiedDiff, resolveWorkspacePath, writeWorkspaceText, withMutation } from "@iyon/plugins";
import { readFile } from "node:fs/promises";
import { renderWriteCall, renderWriteResult } from "./render.ts";

export const writeTool = defineTool({
  name: "write",
  description: "Write content to a file. Creates the file if it doesn't exist, overwrites if it does. Automatically creates parent directories.",
  inputSchema: {
    type: "object",
    properties: { path: { type: "string", description: "Path to the file to write (relative or absolute)" }, content: { type: "string", description: "Content to write to the file" } },
    required: ["path", "content"],
    additionalProperties: false,
  },
  execution: { executionMode: "sequential", approval: "neverAsk", promptSnippet: "Create or overwrite files", promptGuidelines: ["Use write only for new files or complete rewrites."] },
  execute: async (context: ToolContext, args: { path: string; content: string }) => {
    if (!args || typeof args.path !== "string" || typeof args.content !== "string") throw new Error("invalid write input");
    if (!args.path.trim()) throw new Error("write path must not be empty");
    checkCancelled(context);
    const resolved = await resolveWorkspacePath(context.workspace, args.path, "write");
    return await withMutation(resolved, async () => {
      checkCancelled(context);
      const previous = await existingText(context, args.path, resolved);
      await writeWorkspaceText(context.workspace, args.path, args.content);
      checkCancelled(context);
      const details = previous === undefined ? {} : { diff: unifiedDiff(args.path, normalizeToLf(previous), normalizeToLf(args.content)) };
      return { content: [{ type: "text", text: `Successfully wrote ${new TextEncoder().encode(args.content).byteLength} bytes to ${args.path}` }], details, isError: false };
    });
  },
  renderCall: renderWriteCall,
  renderResult: renderWriteResult,
});

async function existingText(context: ToolContext, inputPath: string, resolvedPath: string): Promise<string | undefined> {
  try {
    const bytes = context.workspace.readText ? new TextEncoder().encode(await context.workspace.readText(inputPath)) : await readFile(resolvedPath);
    return new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes);
  } catch (error) {
    if (error instanceof Error && "code" in error && (error as NodeJS.ErrnoException).code === "ENOENT") return "";
    if (error instanceof TypeError) return undefined;
    if (error instanceof Error && /invalid|UTF-8|utf-8/i.test(error.message)) return undefined;
    if (error instanceof Error && "code" in error && (error as NodeJS.ErrnoException).code === "ENOENT") return "";
    return undefined;
  }
}

function normalizeToLf(value: string): string { return value.replaceAll("\r\n", "\n").replaceAll("\r", "\n"); }
function checkCancelled(context: ToolContext): void { if (context.signal.aborted) throw new Error("write tool cancelled"); }

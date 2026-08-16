import { defineTool, type ToolContext } from "@iyon/sdk";
import { DEFAULT_MODEL_MAX_BYTES, DEFAULT_MODEL_MAX_LINES, findProgram, truncateTail } from "@iyon/plugins";
import { writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { bashApprovalPolicy } from "./policy.ts";
import { renderBashCall, renderBashResult } from "./render.ts";

const ROLLING_BUFFER_BYTES = DEFAULT_MODEL_MAX_BYTES * 2;

export const bashTool = defineTool({
  name: "bash",
  description: `Execute a bash command in the current working directory. Returns stdout and stderr. Output is truncated to last ${DEFAULT_MODEL_MAX_LINES} lines or ${DEFAULT_MODEL_MAX_BYTES / 1024}KB (whichever is hit first). If truncated, full output is saved to a temp file. Optionally provide a timeout in seconds.`,
  inputSchema: {
    type: "object",
    properties: { command: { type: "string", description: "Bash command to execute" }, timeout: { type: "number", description: "Timeout in seconds (optional, no default timeout)" } },
    required: ["command"],
    additionalProperties: false,
  },
  execution: { executionMode: "sequential", approval: "neverAsk", promptSnippet: "Execute bash commands (ls, grep, find, etc.)" },
  policy: bashApprovalPolicy,
  execute: async (context: ToolContext, args: { command: string; timeout?: number }) => {
    if (!args || typeof args.command !== "string") throw new Error("invalid bash input");
    if (!args.command.trim()) throw new Error("bash command must not be empty");
    if (context.signal.aborted) throw new Error("bash tool cancelled");
    const output = await runBash(context, args.command, args.timeout);
    const truncated = truncateTail(output.text, { maxLines: DEFAULT_MODEL_MAX_LINES, maxBytes: DEFAULT_MODEL_MAX_BYTES });
    const details: Record<string, unknown> = { exitCode: output.exitCode };
    if (truncated.report.truncated) details.truncation = truncated.report;
    if (output.fullOutputPath) details.fullOutputPath = output.fullOutputPath;
    let text = truncated.text;
    if (output.exitCode !== null && output.exitCode !== 0) text += `${text ? "\n" : ""}[Command exited with code ${output.exitCode}]`;
    return { content: [{ type: "text", text }], details, isError: output.exitCode !== null && output.exitCode !== 0 };
  },
  renderCall: renderBashCall,
  renderResult: renderBashResult,
});

interface BashOutput { text: string; exitCode: number | null; fullOutputPath?: string }

async function runBash(context: ToolContext, commandText: string, timeoutSeconds?: number): Promise<BashOutput> {
  const shell = "/bin/bash" in Bun ? "/bin/bash" : findProgram("bash") ?? "/bin/sh";
  const child = Bun.spawn([shell, "-lc", commandText], { cwd: context.cwd, stdout: "pipe", stderr: "pipe" });
  const stdout = child.stdout;
  const stderr = child.stderr;
  if (!stdout || typeof stdout === "number" || !stderr || typeof stderr === "number") throw new Error("failed to capture bash output");
  const readers = [{ reader: stdout.getReader() }, { reader: stderr.getReader() }];
  const all: Uint8Array[] = [];
  const rolling: Uint8Array[] = [];
  let rollingBytes = 0;
  let processDone = false;
  let exitCode: number | null = null;
  const status = child.exited.then((code) => ({ type: "status" as const, code }));
  const cancel = new Promise<never>((_, reject) => context.signal.addEventListener("abort", () => { child.kill(); reject(new Error("bash command cancelled")); }, { once: true }));
  const timeout = timeoutSeconds && timeoutSeconds > 0 ? new Promise<never>((_, reject) => setTimeout(() => { child.kill(); reject(new Error(`bash command timed out after ${timeoutSeconds}s`)); }, timeoutSeconds * 1000)) : undefined;
  try {
    while (readers.length > 0 || !processDone) {
      const reads = readers.map((entry, index) => entry.reader.read().then((result) => ({ type: "chunk" as const, index, result })));
      const event = await Promise.race([...(reads as Promise<{ type: "chunk"; index: number; result: ReadableStreamReadResult<Uint8Array> }>[]) , ...(processDone ? [] : [status]), cancel, ...(timeout ? [timeout] : [])]);
      if (event.type === "status") { processDone = true; exitCode = event.code; continue; }
      if (event.result.done) { readers.splice(event.index, 1); continue; }
      const chunk = event.result.value;
      all.push(chunk); rolling.push(chunk); rollingBytes += chunk.byteLength;
      while (rollingBytes > ROLLING_BUFFER_BYTES && rolling.length > 1) rollingBytes -= rolling.shift()!.byteLength;
      const update = truncateTail(decode(rolling), { maxLines: 20, maxBytes: 8 * 1024 });
      if (update.text) await context.update({ type: "text", text: update.text });
    }
  } finally {
    context.signal.removeEventListener("abort", () => undefined);
  }
  const text = decode(all);
  let fullOutputPath: string | undefined;
  if (new TextEncoder().encode(text).byteLength > DEFAULT_MODEL_MAX_BYTES) {
    fullOutputPath = join(tmpdir(), `iyon-bash-${Date.now()}-${Math.random().toString(16).slice(2)}.log`);
    await writeFile(fullOutputPath, text, "utf8");
  }
  return { text, exitCode, ...(fullOutputPath ? { fullOutputPath } : {}) };
}

function decode(chunks: readonly Uint8Array[]): string { const bytes = new Uint8Array(chunks.reduce((total, chunk) => total + chunk.byteLength, 0)); let offset = 0; for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; } return new TextDecoder().decode(bytes); }

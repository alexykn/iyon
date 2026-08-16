export interface ProcessSpec {
  readonly program: string;
  readonly args?: readonly string[];
  readonly cwd?: string;
  readonly timeoutMs?: number;
  readonly mergeStderr?: boolean;
}

export interface ProcessOutput {
  readonly stdout: Uint8Array;
  readonly stderr: Uint8Array;
  readonly exitCode: number | null;
}

export function findProgram(name: string): string | undefined {
  const path = process.env.PATH ?? "";
  for (const directory of path.split(":")) {
    if (!directory) continue;
    const candidate = `${directory}/${name}`;
    try {
      accessSync(candidate, constants.X_OK);
      return candidate;
    } catch {
      continue;
    }
  }
  return undefined;
}

export async function runCapture(spec: ProcessSpec, signal?: AbortSignal): Promise<ProcessOutput> {
  if (signal?.aborted) throw new Error("process cancelled before start");
  const command = Bun.spawn([spec.program, ...(spec.args ?? [])], { cwd: spec.cwd, stdout: "pipe", stderr: "pipe" });
  let timeout: ReturnType<typeof setTimeout> | undefined;
  let abort: (() => void) | undefined;
  const cancel = new Promise<never>((_, reject) => {
    abort = () => { command.kill(); reject(new Error("process cancelled")); };
    signal?.addEventListener("abort", abort, { once: true });
    if (spec.timeoutMs !== undefined) timeout = setTimeout(() => { command.kill(); reject(new Error(`process timed out after ${spec.timeoutMs}ms`)); }, spec.timeoutMs);
  });
  try {
    const output = await Promise.race([readProcess(command, spec.mergeStderr), cancel]);
    return output;
  } finally {
    if (timeout !== undefined) clearTimeout(timeout);
    if (abort) signal?.removeEventListener("abort", abort);
  }
}

async function readProcess(command: ReturnType<typeof Bun.spawn>, mergeStderr = false): Promise<ProcessOutput> {
  const stdoutStream = command.stdout;
  const stderrStream = command.stderr;
  if (!stdoutStream || typeof stdoutStream === "number" || !stderrStream || typeof stderrStream === "number") throw new Error("failed to capture process output");
  const [stdout, stderr, exitCode] = await Promise.all([
    new Response(stdoutStream).arrayBuffer().then((value) => new Uint8Array(value)),
    new Response(stderrStream).arrayBuffer().then((value) => new Uint8Array(value)),
    command.exited,
  ]);
  return mergeStderr ? { stdout: concat(stdout, stderr), stderr: new Uint8Array(), exitCode } : { stdout, stderr, exitCode };
}

function concat(left: Uint8Array, right: Uint8Array): Uint8Array { const value = new Uint8Array(left.length + right.length); value.set(left); value.set(right, left.length); return value; }
import { accessSync, constants } from "node:fs";

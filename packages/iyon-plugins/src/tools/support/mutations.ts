import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, isAbsolute, normalize, resolve } from "node:path";
import type { WorkspaceHandle } from "@iyon/sdk";

const pending = new Map<string, Promise<void>>();

export async function withMutation<T>(path: string, operation: () => Promise<T>): Promise<T> {
  const previous = pending.get(path) ?? Promise.resolve();
  let release!: () => void;
  const current = new Promise<void>((resolveRelease) => { release = resolveRelease; });
  pending.set(path, current);
  await previous;
  try { return await operation(); } finally { release(); if (pending.get(path) === current) pending.delete(path); }
}

export async function resolveWorkspacePath(workspace: WorkspaceHandle, path: string, operation: "read" | "write" | "search" = "read"): Promise<string> {
  const resolver = operation === "write" ? workspace.resolveWritePath : operation === "search" ? workspace.resolveSearchPath : workspace.resolveReadPath;
  if (resolver) return await resolver(path);
  const root = workspace.root ?? process.cwd();
  const resolved = normalize(isAbsolute(path) ? path : resolve(root, path));
  const rootPath = normalize(resolve(root));
  if (!resolved.startsWith(`${rootPath}/`) && resolved !== rootPath) throw new Error(`path escapes workspace root: ${path}`);
  return resolved;
}

export async function readWorkspaceText(workspace: WorkspaceHandle, path: string): Promise<string> {
  if (workspace.readText) return await workspace.readText(path);
  return await readFile(await resolveWorkspacePath(workspace, path, "read"), "utf8");
}

export async function writeWorkspaceText(workspace: WorkspaceHandle, path: string, content: string): Promise<void> {
  if (workspace.writeText) { await workspace.writeText(path, content); return; }
  const resolved = await resolveWorkspacePath(workspace, path, "write");
  await mkdir(dirname(resolved), { recursive: true });
  await writeFile(resolved, content, "utf8");
}

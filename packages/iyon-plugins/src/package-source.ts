import { resolve } from "node:path";
import { SourceError } from "./errors.ts";

export type PackageSource =
  | { readonly type: "npm"; readonly name: string; readonly version?: string; readonly descriptor: string }
  | { readonly type: "git"; readonly url: string; readonly ref?: string; readonly descriptor: string }
  | { readonly type: "local"; readonly path: string; readonly descriptor: string };

export type PackageSourceInput =
  | PackageSource
  | string
  | { readonly type: "npm"; readonly name: string; readonly version?: string }
  | { readonly type: "git"; readonly url: string; readonly ref?: string }
  | { readonly type: "local"; readonly path: string };

export function sourceIdentity(source: PackageSource): string {
  return source.descriptor;
}

export function normalizePackageSource(input: PackageSourceInput, packageRoot = process.cwd()): PackageSource {
  if (typeof input === "string") {
    if (input.startsWith("npm:")) return npmSource(input.slice(4));
    if (input.startsWith("git:")) return gitSource(input.slice(4));
    return localSource(input, packageRoot);
  }

  if (!input || typeof input !== "object" || !("type" in input)) {
    throw new SourceError("package source must be npm:, git:, or a local path");
  }
  if (input.type === "npm") return npmSource(input.name, input.version);
  if (input.type === "git") return gitSource(input.url, input.ref);
  if (input.type === "local") return localSource(input.path, packageRoot);
  throw new SourceError(`unsupported package source type: ${String((input as { readonly type?: unknown }).type)}`);
}

function npmSource(name: string, version?: string): PackageSource {
  if (!name || name.startsWith("/") || name.includes("\\")) throw new SourceError(`invalid npm package name: ${name}`);
  const descriptor = `npm:${name}${version ? `@${version}` : ""}`;
  return { type: "npm", name, ...(version ? { version } : {}), descriptor };
}

function gitSource(url: string, ref?: string): PackageSource {
  if (!url || (!url.includes(":") && !url.startsWith("."))) throw new SourceError(`invalid git source: ${url}`);
  return { type: "git", url, ...(ref ? { ref } : {}), descriptor: `git:${url}${ref ? `#${ref}` : ""}` };
}

function localSource(path: string, packageRoot: string): PackageSource {
  if (!path) throw new SourceError("local package source requires a path");
  const normalized = resolve(packageRoot, path);
  return { type: "local", path: normalized, descriptor: `local:${normalized}` };
}

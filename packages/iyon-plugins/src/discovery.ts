import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";
import { ManifestError } from "./errors.ts";
import { LOAD_SCOPE_ORDER, type LoadScope, normalizeManifest, type NormalizedManifest } from "./manifest.ts";
import { normalizePackageSource, type PackageSourceInput } from "./package-source.ts";
import { validateCompatibility, type RuntimeCompatibility } from "./compatibility.ts";

export interface PackageCandidate {
  readonly manifest: NormalizedManifest;
  readonly scope: LoadScope;
  readonly source: NormalizedManifest["source"];
}

export interface DiscoveryOptions {
  readonly bundled?: readonly (string | PackageCandidate | PackageDescriptor)[];
  readonly user?: readonly (string | PackageCandidate | PackageDescriptor)[];
  readonly project?: readonly (string | PackageCandidate | PackageDescriptor)[];
  readonly runtime?: RuntimeCompatibility;
}

export interface PackageDescriptor {
  readonly root: string;
  readonly manifest?: Readonly<Record<string, unknown>>;
  readonly source?: PackageSourceInput;
}

export async function discoverPackages(options: DiscoveryOptions = {}): Promise<PackageCandidate[]> {
  const candidates: PackageCandidate[] = [];
  for (const scope of LOAD_SCOPE_ORDER) {
    const entries = options[scope] ?? [];
    const resolved = await Promise.all(entries.map((entry) => discoverCandidate(entry, scope, options.runtime)));
    candidates.push(...resolved.sort((left, right) => `${left.manifest.packageId}:${left.manifest.source.descriptor}`.localeCompare(`${right.manifest.packageId}:${right.manifest.source.descriptor}`)));
  }
  return candidates;
}

export async function discoverPackageRoot(root: string, scope: LoadScope, source?: PackageSourceInput, runtime?: RuntimeCompatibility): Promise<PackageCandidate> {
  return discoverCandidate({ root, source }, scope, runtime);
}

async function discoverCandidate(entry: string | PackageCandidate | PackageDescriptor, scope: LoadScope, runtime?: RuntimeCompatibility): Promise<PackageCandidate> {
  if (typeof entry !== "string" && "manifest" in entry && "scope" in entry) return entry;
  const descriptor = typeof entry === "string" ? { root: entry } : entry;
  const manifestData = descriptor.manifest ?? await readManifest(descriptor.root);
  const source = descriptor.source ?? { type: "local" as const, path: descriptor.root };
  const manifest = normalizeManifest(manifestData, descriptor.root, source);
  validateCompatibility(manifest, runtime);
  return { manifest, scope, source: normalizePackageSource(source, descriptor.root) };
}

async function readManifest(root: string): Promise<Readonly<Record<string, unknown>>> {
  try {
    return JSON.parse(await readFile(join(root, "package.json"), "utf8")) as Readonly<Record<string, unknown>>;
  } catch (error) {
    throw new ManifestError(`cannot read package manifest from ${root}`, { source: `local:${root}` }, error);
  }
}

export async function discoverDirectory(root: string, scope: LoadScope, runtime?: RuntimeCompatibility): Promise<PackageCandidate[]> {
  const names = (await readdir(root, { withFileTypes: true })).filter((entry) => entry.isDirectory()).map((entry) => entry.name).sort();
  return discoverPackages({ [scope]: names.map((name) => join(root, name)), runtime });
}

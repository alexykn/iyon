import { resolve, relative } from "node:path";
import { ManifestError } from "./errors.ts";
import { normalizePackageSource, type PackageSource, type PackageSourceInput } from "./package-source.ts";

export type LoadScope = "bundled" | "user" | "project";
export const LOAD_SCOPE_ORDER: readonly LoadScope[] = ["bundled", "user", "project"];

export interface CompatibilityRequirements {
  readonly engines?: Readonly<Record<string, string>>;
  readonly iyon?: string;
  readonly api?: string;
  readonly runtime?: string;
}

export interface ExtensionManifest {
  readonly id: string;
  readonly entrypoint: string;
  readonly relativeEntrypoint: string;
}

export interface IyonManifest {
  readonly extensions: string | Readonly<Record<string, string>> | readonly (string | { readonly id: string; readonly entry: string })[];
  readonly catalog?: unknown;
  readonly compatibility?: CompatibilityRequirements;
}

export interface NormalizedManifest {
  readonly packageId: string;
  readonly name: string;
  readonly version: string;
  readonly keywords: readonly string[];
  readonly packageRoot: string;
  readonly source: PackageSource;
  readonly extensions: readonly ExtensionManifest[];
  readonly catalog?: unknown;
  readonly compatibility: CompatibilityRequirements;
  readonly raw: Readonly<Record<string, unknown>>;
}

export interface PackageManifestInput extends Readonly<Record<string, unknown>> {
  readonly name?: string;
  readonly version?: string;
  readonly keywords?: readonly string[];
  readonly "iyon-package"?: unknown;
  readonly iyon?: IyonManifest;
}

export function normalizeManifest(input: PackageManifestInput, packageRoot: string, source?: PackageSourceInput): NormalizedManifest {
  const name = typeof input.name === "string" ? input.name : "";
  const version = typeof input.version === "string" ? input.version : "";
  const keywords = Array.isArray(input.keywords) ? input.keywords.filter((keyword): keyword is string => typeof keyword === "string") : [];
  const iyon = input.iyon;
  if (input["iyon-package"] === undefined && !keywords.includes("iyon-package")) {
    throw new ManifestError(`package ${name || "<unknown>"} is missing the iyon-package discoverability marker`, { packageId: name });
  }
  if (!name || !version || !iyon || typeof iyon !== "object") {
    throw new ManifestError(`invalid manifest for ${name || "<unknown>"}`, { packageId: name });
  }
  const root = resolve(packageRoot);
  const normalizedSource = normalizePackageSource(source ?? { type: "local", path: root }, root);
  const extensions = normalizeExtensions(iyon.extensions, name, root, normalizedSource.descriptor);
  return {
    packageId: name,
    name,
    version,
    keywords,
    packageRoot: root,
    source: normalizedSource,
    extensions,
    ...(iyon.catalog === undefined ? {} : { catalog: iyon.catalog }),
    compatibility: iyon.compatibility ?? {},
    raw: input,
  };
}

function normalizeExtensions(value: IyonManifest["extensions"], packageId: string, root: string, source: string): ExtensionManifest[] {
  if (typeof value === "string") return [extension(packageId, value, root, source)];
  if (Array.isArray(value)) {
    if (value.length === 0) throw new ManifestError(`package ${packageId} declares no extensions`, { packageId, source });
    return value.map((item, index) => {
      if (typeof item === "string") return extension(`${packageId}#${index + 1}`, item, root, source);
      if (!item || typeof item.id !== "string" || typeof item.entry !== "string") throw new ManifestError(`invalid extension at index ${index} in ${packageId}`, { packageId, source });
      return extension(item.id, item.entry, root, source);
    });
  }
  if (!value || typeof value !== "object") throw new ManifestError(`package ${packageId} has malformed iyon.extensions`, { packageId, source });
  const entries = Object.entries(value);
  if (entries.length === 0) throw new ManifestError(`package ${packageId} declares no extensions`, { packageId, source });
  return entries.sort(([left], [right]) => left.localeCompare(right)).map(([id, entry]) => {
    if (typeof entry !== "string") throw new ManifestError(`extension ${id} in ${packageId} must be a string path`, { packageId, extensionId: id, source });
    return extension(id, entry, root, source);
  });
}

function extension(id: string, entrypoint: string, root: string, source: string): ExtensionManifest {
  if (!id || !entrypoint || entrypoint.startsWith("#") || entrypoint.includes("\0")) throw new ManifestError(`invalid extension ${id || "<unknown>"}`, { packageId: id, source });
  const absolute = resolve(root, entrypoint);
  const relativeEntrypoint = relative(root, absolute);
  if (relativeEntrypoint.startsWith("..") || relativeEntrypoint.includes("\0")) throw new ManifestError(`extension ${id} escapes package root`, { packageId: id, extensionId: id, source, entrypoint });
  return { id, entrypoint: absolute, relativeEntrypoint: `./${relativeEntrypoint.replaceAll("\\", "/")}` };
}

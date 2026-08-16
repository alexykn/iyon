import { CompatibilityError } from "./errors.ts";
import type { NormalizedManifest } from "./manifest.ts";

export interface RuntimeCompatibility {
  readonly version: string;
  readonly apiVersion?: string;
  readonly runtimeVersion?: string;
  readonly engines?: Readonly<Record<string, string>>;
}

export function validateCompatibility(manifest: NormalizedManifest, runtime: RuntimeCompatibility = { version: "0.1.0" }): void {
  const requirements = manifest.compatibility;
  for (const [engine, range] of Object.entries(requirements.engines ?? {})) {
    const actual = runtime.engines?.[engine];
    if (actual === undefined || !satisfies(actual, range)) fail(manifest, `requires ${engine} ${range}, runtime has ${actual ?? "none"}`);
  }
  if (requirements.iyon && !satisfies(runtime.version, requirements.iyon)) fail(manifest, `requires Iyon ${requirements.iyon}, runtime has ${runtime.version}`);
  if (requirements.api && !satisfies(runtime.apiVersion ?? runtime.version, requirements.api)) fail(manifest, `requires Iyon API ${requirements.api}, runtime has ${runtime.apiVersion ?? runtime.version}`);
  if (requirements.runtime && !satisfies(runtime.runtimeVersion ?? runtime.version, requirements.runtime)) fail(manifest, `requires Iyon runtime ${requirements.runtime}, runtime has ${runtime.runtimeVersion ?? runtime.version}`);
}

export function satisfies(version: string, range: string): boolean {
  if (!range || range === "*" || range === "latest") return true;
  const actual = parseVersion(version);
  return range.split("||").some((alternative) => alternative.trim().split(/\s+/).filter(Boolean).every((part) => satisfiesPart(actual, part)));
}

function satisfiesPart(actual: [number, number, number], part: string): boolean {
  const operator = part.match(/^(\^|~|>=|<=|>|<|=)?\s*(\d+(?:\.\d+)?(?:\.\d+)?)$/);
  if (!operator) return false;
  const expected = parseVersion(operator[2]);
  const comparison = compare(actual, expected);
  switch (operator[1]) {
    case "^": return actual[0] === expected[0] && comparison >= 0;
    case "~": return actual[0] === expected[0] && actual[1] === expected[1] && comparison >= 0;
    case ">=": return comparison >= 0;
    case "<=": return comparison <= 0;
    case ">": return comparison > 0;
    case "<": return comparison < 0;
    default: return comparison === 0;
  }
}

function parseVersion(value: string): [number, number, number] {
  const match = value.replace(/^v/, "").match(/^(\d+)(?:\.(\d+))?(?:\.(\d+))?/);
  return [Number(match?.[1] ?? -1), Number(match?.[2] ?? 0), Number(match?.[3] ?? 0)];
}

function compare(left: [number, number, number], right: [number, number, number]): number {
  return left[0] - right[0] || left[1] - right[1] || left[2] - right[2];
}

function fail(manifest: NormalizedManifest, reason: string): never {
  throw new CompatibilityError(`incompatible package ${manifest.packageId} from ${manifest.source.descriptor}: ${reason}`, { packageId: manifest.packageId, source: manifest.source.descriptor });
}

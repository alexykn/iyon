import { LoadError, diagnosticMessage } from "./errors.ts";
import type { PackageCandidate } from "./discovery.ts";

export interface LoadFailure {
  readonly ok: false;
  readonly packageId: string;
  readonly extensionId: string;
  readonly source: string;
  readonly error: unknown;
}

export interface LoadSuccess {
  readonly ok: true;
  readonly packageId: string;
  readonly extensionId: string;
  readonly generation: number;
  readonly source: PackageCandidate["source"];
}

export type LoadResult = LoadSuccess | LoadFailure;

export function asLoadError(result: LoadFailure): LoadError {
  return result.error instanceof LoadError ? result.error : new LoadError(`failed to load ${result.packageId}/${result.extensionId} from ${result.source}: ${diagnosticMessage(result.error)}`, { packageId: result.packageId, extensionId: result.extensionId, source: result.source }, result.error);
}

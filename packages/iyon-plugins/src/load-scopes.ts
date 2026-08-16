import { discoverPackages, type DiscoveryOptions, type PackageCandidate } from "./discovery.ts";
import { PackageLoader } from "./loader.ts";

export interface ScopeLoaderOptions extends DiscoveryOptions {
  readonly loader?: PackageLoader;
}

export async function loadScopes(options: ScopeLoaderOptions = {}): Promise<{ readonly candidates: readonly PackageCandidate[]; readonly loaded: readonly import("./load-errors.ts").LoadResult[]; readonly failures: readonly import("./load-errors.ts").LoadFailure[] }> {
  const candidates = await discoverPackages(options);
  const loader = options.loader ?? new PackageLoader();
  const result = await loader.loadAll(candidates);
  return { candidates, ...result };
}

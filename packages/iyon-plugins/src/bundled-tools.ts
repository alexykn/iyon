import { installIyonVirtualModules } from "@iyon/runtime";
import { discoverPackages, type PackageCandidate } from "./discovery.ts";
import { PackageLoader } from "./loader.ts";
import type { RuntimeRegistries } from "./extension-api.ts";

const bundledToolRoots = ["bash", "read", "write", "edit", "grep", "find", "ls"].map((name) => new URL(`../../../plugins/tools/${name}/`, import.meta.url).pathname);

export async function discoverBundledToolPackages(): Promise<readonly PackageCandidate[]> { return discoverPackages({ bundled: bundledToolRoots }); }

export async function registerBundledTools(options: { readonly registries?: RuntimeRegistries } = {}): Promise<PackageLoader> {
  installIyonVirtualModules();
  const loader = new PackageLoader({ ...(options.registries ? { registries: options.registries } : {}) });
  const result = await loader.loadAll(await discoverBundledToolPackages());
  if (result.failures.length > 0) throw new AggregateError(result.failures.map((failure) => failure.error), "bundled tool registration failed");
  return loader;
}

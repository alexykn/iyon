import { installIyonVirtualModules } from "../virtual-modules.ts";
import { discoverPackages, type PackageCandidate } from "@iyon/plugins";
import { PackageLoader, type RuntimeRegistries } from "@iyon/plugins";

const bundledRoots = [
  new URL("../../../../plugins/providers/mock/", import.meta.url).pathname,
  new URL("../../../../plugins/providers/openrouter/", import.meta.url).pathname,
  new URL("../../../../plugins/providers/openai-codex/", import.meta.url).pathname,
] as const;

export async function discoverBundledProviderPackages(): Promise<readonly PackageCandidate[]> {
  return discoverPackages({ bundled: bundledRoots });
}

export async function registerBundledProviders(options: { readonly registries?: RuntimeRegistries } = {}): Promise<PackageLoader> {
  installIyonVirtualModules();
  const loader = new PackageLoader({ ...(options.registries ? { registries: options.registries } : {}) });
  const result = await loader.loadAll(await discoverBundledProviderPackages());
  if (result.failures.length > 0) throw new AggregateError(result.failures.map((failure) => failure.error), "bundled provider registration failed");
  return loader;
}

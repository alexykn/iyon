import { EventHub } from "./events.ts";
import { AppRegistry } from "./app-registry.ts";
import { SceneExtensions } from "./scene-extensions.ts";
import { ToolRegistry, ProviderRegistry, AgentRegistry, CommandRegistry, ShortcutRegistry } from "./registries.ts";
import type { RuntimeRegistries } from "./extension-api.ts";
import type { PackageCandidate } from "./discovery.ts";
import { activateExtension, unloadExtension, type ActivationRecord, type ActivationRuntime } from "./activation.ts";
import type { LoadFailure, LoadResult } from "./load-errors.ts";
import { asLoadError } from "./load-errors.ts";
import { isIyonVirtualModulesInstalled } from "@iyon/runtime";

export interface PluginRuntimeOptions extends ActivationRuntime {
  readonly registries: RuntimeRegistries;
}

export function createRegistries(): RuntimeRegistries {
  let generation = 0;
  const nextGeneration = () => ++generation;
  return {
    tools: new ToolRegistry({ name: "tool", nextGeneration }),
    providers: new ProviderRegistry({ name: "provider", nextGeneration }),
    agents: new AgentRegistry({ name: "agent", nextGeneration }),
    apps: new AppRegistry({ nextGeneration }),
    commands: new CommandRegistry({ name: "command", nextGeneration }),
    shortcuts: new ShortcutRegistry({ name: "shortcut", nextGeneration }),
    scene: new SceneExtensions({ nextGeneration }),
  };
}

export class PackageLoader {
  readonly registries: RuntimeRegistries;
  readonly events: EventHub;
  private readonly runtime: ActivationRuntime;
  private readonly active = new Map<string, ActivationRecord>();
  private generation = 0;

  constructor(options: Partial<PluginRuntimeOptions> = {}) {
    this.registries = options.registries ?? createRegistries();
    this.events = options.events ?? new EventHub();
    this.runtime = { registries: this.registries, events: this.events, nextGeneration: options.nextGeneration ?? (() => ++this.generation), compatibility: options.compatibility };
  }

  async load(candidate: PackageCandidate): Promise<LoadResult[]> {
    if (!isIyonVirtualModulesInstalled()) {
      const failure: LoadFailure = { ok: false, packageId: candidate.manifest.packageId, extensionId: candidate.manifest.extensions[0]?.id ?? "<unknown>", source: candidate.source.descriptor, error: new Error("Iyon virtual modules are not installed; call installIyonVirtualModules() before loading extensions") };
      return [failure];
    }
    const results: LoadResult[] = [];
    for (const extension of candidate.manifest.extensions) {
      try {
        const record = await activateExtension(candidate, extension.id, this.runtime);
        this.active.set(this.key(candidate.manifest.packageId, extension.id), record);
        results.push(record.result);
      } catch (error) {
        results.push({ ok: false, packageId: candidate.manifest.packageId, extensionId: extension.id, source: candidate.source.descriptor, error: asLoadError({ ok: false, packageId: candidate.manifest.packageId, extensionId: extension.id, source: candidate.source.descriptor, error }) });
      }
    }
    return results;
  }

  async loadOrThrow(candidate: PackageCandidate): Promise<LoadResult[]> {
    const results = await this.load(candidate);
    const failure = results.find((result): result is LoadFailure => !result.ok);
    if (failure) throw asLoadError(failure);
    return results;
  }

  async loadAll(candidates: readonly PackageCandidate[]): Promise<{ readonly loaded: readonly LoadResult[]; readonly failures: readonly LoadFailure[] }> {
    const loaded: LoadResult[] = [];
    const failures: LoadFailure[] = [];
    for (const candidate of candidates) {
      for (const result of await this.load(candidate)) {
        if (result.ok) loaded.push(result);
        else failures.push(result);
      }
    }
    return { loaded, failures };
  }

  async unload(packageId: string, extensionId?: string): Promise<void> {
    const records = [...this.active.entries()].filter(([key]) => key.startsWith(`${packageId}/`) && (extensionId === undefined || key === this.key(packageId, extensionId)));
    for (const [key, record] of records.reverse()) { await unloadExtension(record, this.runtime); this.active.delete(key); }
  }

  get activeExtensions(): readonly LoadResult[] { return [...this.active.values()].map((record) => record.result); }
  private key(packageId: string, extensionId: string): string { return `${packageId}/${extensionId}`; }
}

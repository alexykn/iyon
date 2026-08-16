import { ActivationError, diagnosticMessage } from "./errors.ts";
import { DisposableStack, asDisposable, type Disposable } from "./disposable.ts";
import { EventHub } from "./events.ts";
import { ExtensionContext, type ExtensionIdentity, type RuntimeRegistries } from "./extension-api.ts";
import { importExtension } from "./extension-module.ts";
import type { PackageCandidate } from "./discovery.ts";
import type { LoadResult, LoadSuccess } from "./load-errors.ts";
import { validateCompatibility, type RuntimeCompatibility } from "./compatibility.ts";

export interface ActivationRuntime {
  readonly registries: RuntimeRegistries;
  readonly events: EventHub;
  readonly nextGeneration: () => number;
  readonly compatibility?: RuntimeCompatibility;
}

export interface ActivationRecord {
  readonly result: LoadSuccess;
  readonly packageId: string;
  readonly extensionId: string;
  readonly resources: DisposableStack;
  readonly contributions: readonly any[];
}

export async function activateExtension(candidate: PackageCandidate, extensionId: string, runtime: ActivationRuntime): Promise<ActivationRecord> {
  validateCompatibility(candidate.manifest, runtime.compatibility);
  const extension = candidate.manifest.extensions.find((item) => item.id === extensionId);
  if (!extension) throw new ActivationError(`extension ${candidate.manifest.packageId}/${extensionId} is not declared by its manifest`, { packageId: candidate.manifest.packageId, extensionId, source: candidate.source.descriptor });
  const identity: ExtensionIdentity = { packageId: candidate.manifest.packageId, extensionId, scope: candidate.scope, source: candidate.source, generation: runtime.nextGeneration() };
  const resources = new DisposableStack();
  const pending: Array<{ readonly type: "registration" | "replacement"; readonly contribution: any }> = [];
  const context = new ExtensionContext(identity, runtime.registries, runtime.events, (registryName, value, options) => {
    const registry = runtime.registries[registryName] as any;
    const active = registry.lookup(value.id);
    const contribution = registry.registerOwned(value, {
      packageId: identity.packageId,
      extensionId: identity.extensionId,
      scope: identity.scope,
      source: identity.source,
      generation: runtime.nextGeneration(),
    }, options);
    pending.push({ type: active ? "replacement" : "registration", contribution });
    return contribution.dispose;
  });

  try {
    const module = await importExtension(extension.entrypoint, { packageId: identity.packageId, extensionId: identity.extensionId });
    const returned = await module.activate(context);
    if (returned) resources.use(asDisposable(returned));
    for (const resource of context.ownedResources) resources.use(resource);
    for (const change of pending) runtime.events.emit(change.type, change);
    runtime.events.emit("activation", { packageId: identity.packageId, extensionId: identity.extensionId, source: { packageId: identity.packageId, extensionId: identity.extensionId, registrationId: `${identity.packageId}/${identity.extensionId}`, generation: identity.generation, scope: identity.scope, source: identity.source } });
    return { result: { ok: true, packageId: identity.packageId, extensionId: identity.extensionId, generation: identity.generation, source: identity.source }, packageId: identity.packageId, extensionId: identity.extensionId, resources, contributions: pending.map((change) => change.contribution) };
  } catch (error) {
    const rollbackErrors: unknown[] = [];
    try { await resources.dispose(); } catch (disposeError) { rollbackErrors.push(disposeError); }
    // Registrations are added to context after resources is built; clean them up even when activation throws early.
    for (const resource of [...context.ownedResources].reverse()) {
      try { await resource.dispose(); } catch (disposeError) { rollbackErrors.push(disposeError); }
    }
    runtime.events.emit("activation-failure", { packageId: identity.packageId, extensionId: identity.extensionId, source: identity.source.descriptor, error });
    if (rollbackErrors.length > 0) throw new ActivationError(`activation and rollback failed for ${identity.packageId}/${identity.extensionId}: ${diagnosticMessage(error)}`, { packageId: identity.packageId, extensionId: identity.extensionId, source: identity.source.descriptor }, new AggregateError([error, ...rollbackErrors]));
    if (error instanceof ActivationError) throw error;
    throw new ActivationError(`activation failed for ${identity.packageId}/${identity.extensionId}: ${diagnosticMessage(error)}`, { packageId: identity.packageId, extensionId: identity.extensionId, source: identity.source.descriptor }, error);
  }
}

export async function unloadExtension(record: ActivationRecord, runtime: ActivationRuntime): Promise<void> {
  await record.resources.dispose();
  for (const contribution of [...record.contributions].reverse()) runtime.events.emit("unload", { contribution });
}

import { RegistrationError } from "./errors.ts";
import type { Disposable } from "./disposable.ts";
import type { ContributionValue, InternalRegistration, RegisteredContribution, RegistrationOptions, SourceMetadata } from "./contributions.ts";
import type { LoadScope } from "./manifest.ts";
import type { PackageSource } from "./package-source.ts";

export interface RegistryOptions {
  readonly name?: string;
  readonly nextGeneration?: () => number;
  readonly onChange?: (change: RegistryChange<ContributionValue>) => void;
}

export type RegistryChange<T extends ContributionValue> =
  | { readonly type: "register" | "replace"; readonly contribution: RegisteredContribution<T> }
  | { readonly type: "unload"; readonly contribution: RegisteredContribution<T> };

interface Layer<T extends ContributionValue> {
  readonly contribution: RegisteredContribution<T>;
  active: boolean;
}

export class LayeredRegistry<T extends ContributionValue> {
  private readonly layers = new Map<string, Layer<T>[]>();
  private generation = 0;
  private readonly registryName: string;
  private readonly allocateGeneration: () => number;
  private readonly onChange?: (change: RegistryChange<T>) => void;

  constructor(options: RegistryOptions = {}) {
    this.registryName = options.name ?? "contribution";
    this.allocateGeneration = options.nextGeneration ?? (() => ++this.generation);
    this.onChange = options.onChange as ((change: RegistryChange<T>) => void) | undefined;
  }

  register(value: T, options?: RegistrationOptions): Disposable;
  register(internal: InternalRegistration<T>): RegisteredContribution<T>;
  register(valueOrInternal: T | InternalRegistration<T>, options: RegistrationOptions = {}): Disposable | RegisteredContribution<T> {
    const internal = isInternal(valueOrInternal) ? valueOrInternal : undefined;
    const value = internal?.value ?? valueOrInternal as T;
    const registrationOptions = internal?.options ?? options;
    validateContribution(value, this.registryName);
    const existing = this.active(value.id);
    if (existing && !registrationOptions.replace) throw new RegistrationError(`duplicate ${this.registryName} id ${value.id}; set replace: true to create a new layer`, { packageId: existing.contribution.source.packageId, extensionId: existing.contribution.source.extensionId, source: existing.contribution.source.source.descriptor });
    const generation = internal?.source.generation ?? this.allocateGeneration();
    const source: SourceMetadata = {
      packageId: internal?.source.packageId ?? "direct",
      extensionId: internal?.source.extensionId ?? "direct",
      registrationId: internal?.source.registrationId ?? `${this.registryName}:${value.id}`,
      generation,
      scope: internal?.source.scope ?? "project",
      source: internal?.source.source ?? { type: "local", path: "<direct>", descriptor: "local:<direct>" },
    };
    const contribution = {} as RegisteredContribution<T>;
    let disposed = false;
    const disposable: Disposable = { dispose: () => { if (disposed) return; disposed = true; this.remove(source.generation, source.registrationId); } };
    Object.assign(contribution, { value, source, generation, id: value.id, dispose: disposable });
    const layers = this.layers.get(value.id) ?? [];
    layers.push({ contribution, active: true });
    this.layers.set(value.id, layers);
    this.onChange?.({ type: existing ? "replace" : "register", contribution });
    return internal ? contribution : disposable;
  }

  lookup(id: string): RegisteredContribution<T> | undefined { return this.active(id)?.contribution; }
  get(id: string): T | undefined { return this.lookup(id)?.value; }
  list(): readonly RegisteredContribution<T>[] { return [...this.layers.values()].map((layers) => layers.find((layer) => layer.active)?.contribution).filter((value): value is RegisteredContribution<T> => value !== undefined).sort((left, right) => left.id.localeCompare(right.id)); }
  snapshot(): readonly RegisteredContribution<T>[] { return this.list(); }
  generations(id: string): readonly number[] { return (this.layers.get(id) ?? []).filter((layer) => layer.active).map((layer) => layer.contribution.generation); }

  remove(generation: number, registrationId?: string): void {
    for (const [id, layers] of this.layers) {
      const layer = layers.find((candidate) => candidate.active && candidate.contribution.generation === generation && (registrationId === undefined || candidate.contribution.source.registrationId === registrationId));
      if (!layer) continue;
      layer.active = false;
      this.onChange?.({ type: "unload", contribution: layer.contribution });
      if (layers.every((candidate) => !candidate.active)) this.layers.delete(id);
      return;
    }
  }

  removeOwned(packageId: string, extensionId: string): readonly RegisteredContribution<T>[] {
    const removed: RegisteredContribution<T>[] = [];
    for (const layers of this.layers.values()) {
      for (const layer of layers) {
        if (layer.active && layer.contribution.source.packageId === packageId && layer.contribution.source.extensionId === extensionId) {
          layer.active = false;
          removed.push(layer.contribution);
          this.onChange?.({ type: "unload", contribution: layer.contribution });
        }
      }
    }
    return removed;
  }

  private active(id: string): Layer<T> | undefined {
    const layers = this.layers.get(id);
    return layers?.findLast((layer) => layer.active);
  }
}

function isInternal<T extends ContributionValue>(value: T | InternalRegistration<T>): value is InternalRegistration<T> {
  return typeof value === "object" && value !== null && "value" in value && "source" in value;
}

function validateContribution<T extends ContributionValue>(value: T, name: string): void {
  if (!value || typeof value !== "object" || typeof value.id !== "string" || value.id.length === 0) throw new RegistrationError(`${name} contributions require a non-empty id`);
}

export type RegistrySource = Pick<SourceMetadata, "packageId" | "extensionId" | "scope" | "source">;
export type RegistryScope = LoadScope;
export type RegistryPackageSource = PackageSource;

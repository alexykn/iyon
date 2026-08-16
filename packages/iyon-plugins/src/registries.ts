import type { ContributionValue, RegistrationOptions, RegisteredContribution, ToolContribution, ProviderContribution, AgentContribution, AppContribution, CommandContribution, ShortcutContribution } from "./contributions.ts";
import { LayeredRegistry, type RegistryChange, type RegistryOptions } from "./registry.ts";
import type { Disposable } from "./disposable.ts";
import type { SourceMetadata } from "./contributions.ts";

export class ContributionRegistry<T extends ContributionValue> {
  protected readonly registry: LayeredRegistry<T>;
  constructor(options: RegistryOptions = {}) { this.registry = new LayeredRegistry<T>(options); }
  register(value: T, options?: RegistrationOptions): Disposable { return this.registry.register(value, options); }
  registerOwned(value: T, source: Omit<SourceMetadata, "generation" | "registrationId"> & Partial<Pick<SourceMetadata, "generation" | "registrationId">>, options?: RegistrationOptions): RegisteredContribution<T> { return this.registry.register({ value, source, options }); }
  lookup(id: string): RegisteredContribution<T> | undefined { return this.registry.lookup(id); }
  get(id: string): T | undefined { return this.registry.get(id); }
  list(): readonly RegisteredContribution<T>[] { return this.registry.list(); }
  snapshot(): readonly RegisteredContribution<T>[] { return this.registry.snapshot(); }
  remove(generation: number, registrationId?: string): void { this.registry.remove(generation, registrationId); }
  removeOwned(packageId: string, extensionId: string): readonly RegisteredContribution<T>[] { return this.registry.removeOwned(packageId, extensionId); }
}

export class ToolRegistry extends ContributionRegistry<ToolContribution> { }
export class ProviderRegistry extends ContributionRegistry<ProviderContribution> { }
export class AgentRegistry extends ContributionRegistry<AgentContribution> { }
export class CommandRegistry extends ContributionRegistry<CommandContribution> { }
export class ShortcutRegistry extends ContributionRegistry<ShortcutContribution> { }

export type AnyRegistryChange = RegistryChange<ContributionValue>;
export type { Disposable, RegisteredContribution, RegistrationOptions, SourceMetadata };

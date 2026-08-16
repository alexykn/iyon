import type { Disposable } from "./disposable.ts";
import type { ExtensionEvents, ExtensionHandler, EventHub } from "./events.ts";
import type { LoadScope } from "./manifest.ts";
import type { PackageSource } from "./package-source.ts";
import type { SourceMetadata, RegistrationOptions, ToolContribution, ProviderContribution, AgentContribution, AppContribution, CommandContribution, ShortcutContribution } from "./contributions.ts";
import { ToolRegistry, ProviderRegistry, AgentRegistry, CommandRegistry, ShortcutRegistry } from "./registries.ts";
import { AppRegistry } from "./app-registry.ts";
import { SceneExtensions, type SceneComposer, type SceneReplacer, type SceneExtensionContext } from "./scene-extensions.ts";

export interface ExtensionIdentity {
  readonly packageId: string;
  readonly extensionId: string;
  readonly scope: LoadScope;
  readonly source: PackageSource;
  readonly generation: number;
}

export interface ExtensionAPI {
  readonly tools: ToolRegistry;
  readonly providers: ProviderRegistry;
  readonly agents: AgentRegistry;
  readonly apps: AppRegistry;
  readonly commands: CommandRegistry;
  readonly shortcuts: ShortcutRegistry;
  readonly scene: SceneExtensions;
  on<E extends keyof ExtensionEvents>(event: E, handler: ExtensionHandler<E>): Disposable;
}

export interface RuntimeRegistries {
  readonly tools: ToolRegistry;
  readonly providers: ProviderRegistry;
  readonly agents: AgentRegistry;
  readonly apps: AppRegistry;
  readonly commands: CommandRegistry;
  readonly shortcuts: ShortcutRegistry;
  readonly scene: SceneExtensions;
}

export class ExtensionContext implements ExtensionAPI {
  readonly tools: ToolRegistry;
  readonly providers: ProviderRegistry;
  readonly agents: AgentRegistry;
  readonly apps: AppRegistry;
  readonly commands: CommandRegistry;
  readonly shortcuts: ShortcutRegistry;
  readonly scene: SceneExtensions;
  private readonly resources: Disposable[] = [];

  constructor(private readonly identity: ExtensionIdentity, registries: RuntimeRegistries, private readonly events: EventHub, private readonly register: (registry: keyof RuntimeRegistries, value: any, options?: RegistrationOptions) => Disposable) {
    this.tools = scopedRegistry(registries.tools, "tools", identity, register, this.resources);
    this.providers = scopedRegistry(registries.providers, "providers", identity, register, this.resources);
    this.agents = scopedRegistry(registries.agents, "agents", identity, register, this.resources);
    this.apps = scopedRegistry(registries.apps, "apps", identity, register, this.resources) as AppRegistry;
    this.commands = scopedRegistry(registries.commands, "commands", identity, register, this.resources) as CommandRegistry;
    this.shortcuts = scopedRegistry(registries.shortcuts, "shortcuts", identity, register, this.resources) as ShortcutRegistry;
    this.scene = scopedScene(registries.scene, identity, register, this.resources);
  }

  on<E extends keyof ExtensionEvents>(event: E, handler: ExtensionHandler<E>): Disposable {
    const subscription = this.events.on(event, handler);
    this.resources.push(subscription);
    return subscription;
  }

  get ownedResources(): readonly Disposable[] { return this.resources; }
}

function scopedRegistry(registry: any, name: keyof RuntimeRegistries, identity: ExtensionIdentity, register: ExtensionContext["register"], resources: Disposable[]): any {
  return {
    register(value: any, options?: RegistrationOptions) {
      const disposable = register(name, value, options);
      resources.push(disposable);
      return disposable;
    },
    registerOwned: undefined,
    lookup: registry.lookup.bind(registry),
    get: registry.get.bind(registry),
    list: registry.list.bind(registry),
    snapshot: registry.snapshot.bind(registry),
    identity,
  };
}

function scopedScene(registry: SceneExtensions, identity: ExtensionIdentity, register: ExtensionContext["register"], resources: Disposable[]): SceneExtensions {
  return {
    compose(value: { id: string; compose: SceneComposer; order?: number }, options?: RegistrationOptions) { const disposable = register("scene", { ...value, kind: "compose" }, options); resources.push(disposable); return disposable; },
    replace(value: { id: string; replace: SceneReplacer; order?: number }, options?: RegistrationOptions) { const disposable = register("scene", { ...value, kind: "replace" }, options); resources.push(disposable); return disposable; },
    register(value: any, options?: RegistrationOptions) { const disposable = register("scene", value, options); resources.push(disposable); return disposable; },
    list: registry.list.bind(registry),
    lookup: registry.lookup.bind(registry),
    apply: registry.apply.bind(registry),
    removeOwned: registry.removeOwned.bind(registry),
    identity,
  } as unknown as SceneExtensions;
}

export type ContributionContract = ToolContribution | ProviderContribution | AgentContribution | AppContribution | CommandContribution | ShortcutContribution;
export type SceneContext = SceneExtensionContext;

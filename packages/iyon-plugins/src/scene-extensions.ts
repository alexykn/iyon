/** The T5 Scene boundary; its body/history remain owned by iyon:tui. */
export interface T5Scene { readonly body: unknown; readonly history?: unknown }
import { LayeredRegistry } from "./registry.ts";
import type { ContributionValue, RegisteredContribution, RegistrationOptions, SourceMetadata } from "./contributions.ts";
import type { Disposable } from "./disposable.ts";

export interface SceneExtensionContext {
  readonly appId?: string;
  readonly [key: string]: unknown;
}

export type SceneComposer = (scene: T5Scene, context: SceneExtensionContext) => T5Scene | Promise<T5Scene>;
export type SceneReplacer = (context: SceneExtensionContext) => T5Scene | Promise<T5Scene>;
export interface SceneComposerRegistration { readonly id: string; readonly compose: SceneComposer; readonly order?: number }
export interface SceneReplacerRegistration { readonly id: string; readonly replace: SceneReplacer; readonly order?: number }

export type SceneExtensionContribution = ContributionValue & (
  | { readonly kind: "compose"; readonly compose: SceneComposer; readonly order?: number }
  | { readonly kind: "replace"; readonly replace: SceneReplacer; readonly order?: number }
);

export class SceneExtensions {
  private readonly registry: LayeredRegistry<SceneExtensionContribution>;
  constructor(options: { readonly nextGeneration?: () => number; readonly onChange?: (change: any) => void } = {}) { this.registry = new LayeredRegistry({ ...options, name: "scene" }); }

  register(value: SceneExtensionContribution, options?: RegistrationOptions): Disposable;
  register(value: SceneExtensionContribution, options?: RegistrationOptions): Disposable { return this.registry.register(value, options) as Disposable; }
  registerOwned(value: SceneExtensionContribution, source: Omit<SourceMetadata, "generation" | "registrationId"> & Partial<Pick<SourceMetadata, "generation" | "registrationId">>, options?: RegistrationOptions): RegisteredContribution<SceneExtensionContribution> { return this.registry.register({ value, source, options }); }

  compose(value: SceneComposerRegistration, options?: RegistrationOptions): Disposable;
  compose(value: SceneComposerRegistration, source: Omit<SourceMetadata, "generation" | "registrationId"> & Partial<Pick<SourceMetadata, "generation" | "registrationId">>, options?: RegistrationOptions): RegisteredContribution<SceneExtensionContribution>;
  compose(value: SceneComposerRegistration, sourceOrOptions: RegistrationOptions | Omit<SourceMetadata, "generation" | "registrationId"> & Partial<Pick<SourceMetadata, "generation" | "registrationId">> = {}, options?: RegistrationOptions): Disposable | RegisteredContribution<SceneExtensionContribution> {
    const contribution = { ...value, kind: "compose" as const };
    return "packageId" in sourceOrOptions ? this.registerOwned(contribution, sourceOrOptions, options) : this.register(contribution, sourceOrOptions);
  }

  replace(value: SceneReplacerRegistration, options?: RegistrationOptions): Disposable;
  replace(value: SceneReplacerRegistration, source: Omit<SourceMetadata, "generation" | "registrationId"> & Partial<Pick<SourceMetadata, "generation" | "registrationId">>, options?: RegistrationOptions): RegisteredContribution<SceneExtensionContribution>;
  replace(value: SceneReplacerRegistration, sourceOrOptions: RegistrationOptions | Omit<SourceMetadata, "generation" | "registrationId"> & Partial<Pick<SourceMetadata, "generation" | "registrationId">> = {}, options?: RegistrationOptions): Disposable | RegisteredContribution<SceneExtensionContribution> {
    const contribution = { ...value, kind: "replace" as const };
    return "packageId" in sourceOrOptions ? this.registerOwned(contribution, sourceOrOptions, options) : this.register(contribution, sourceOrOptions);
  }

  list(): readonly RegisteredContribution<SceneExtensionContribution>[] { return this.registry.list(); }
  lookup(id: string): RegisteredContribution<SceneExtensionContribution> | undefined { return this.registry.lookup(id); }
  removeOwned(packageId: string, extensionId: string): readonly RegisteredContribution<SceneExtensionContribution>[] { return this.registry.removeOwned(packageId, extensionId); }

  async apply(scene: T5Scene, context: SceneExtensionContext = {}): Promise<T5Scene> {
    let current = scene;
    const extensions = [...this.list()].sort((left, right) => (left.value.order ?? 0) - (right.value.order ?? 0) || left.generation - right.generation);
    for (const extension of extensions) {
      try {
        current = extension.value.kind === "compose" ? await extension.value.compose(current, context) : await extension.value.replace(context);
      } catch (error) {
        throw new SceneCompositionError(extension.source, error);
      }
      if (!current || typeof current !== "object" || !("body" in current)) throw new SceneCompositionError(extension.source, new Error("Scene extension did not return a T5 Scene"));
    }
    return current;
  }
}

export class SceneCompositionError extends Error {
  readonly source: SourceMetadata;
  constructor(source: SourceMetadata, cause: unknown) { super(`Scene extension ${source.packageId}/${source.extensionId} failed: ${cause instanceof Error ? cause.message : String(cause)}`, { cause }); this.name = "SceneCompositionError"; this.source = source; }
}

import type { Disposable } from "./disposable.ts";
import type { LoadScope } from "./manifest.ts";
import type { PackageSource } from "./package-source.ts";
import type { ProviderDefinition } from "@iyon/sdk";

export interface SourceMetadata {
  readonly packageId: string;
  readonly extensionId: string;
  readonly registrationId: string;
  readonly generation: number;
  readonly scope: LoadScope;
  readonly source: PackageSource;
}

export interface ContributionValue {
  readonly id: string;
  readonly [key: string]: unknown;
}

export interface RegisteredContribution<T extends ContributionValue> {
  readonly value: T;
  readonly source: SourceMetadata;
  readonly generation: number;
  readonly id: string;
  readonly dispose: Disposable;
}

export interface RegistrationOptions {
  readonly replace?: boolean;
}

export interface InternalRegistration<T extends ContributionValue> {
  readonly value: T;
  readonly source: Omit<SourceMetadata, "generation" | "registrationId"> & Partial<Pick<SourceMetadata, "generation" | "registrationId">>;
  readonly options?: RegistrationOptions;
}

export type ToolContribution = ContributionValue;
// Existing third-party contributions may still register metadata-only
// providers; the runtime selection seam validates the executable fields when
// it resolves a provider.
export type ProviderContribution = ContributionValue & Partial<ProviderDefinition>;
export type AgentContribution = ContributionValue & { readonly create?: (context: unknown) => unknown };
export type AppContribution = ContributionValue & { readonly create: (context: unknown) => unknown };
export type CommandContribution = ContributionValue & { readonly run?: (...args: readonly unknown[]) => unknown };
export type ShortcutContribution = ContributionValue & { readonly keys?: string | readonly string[]; readonly run?: (...args: readonly unknown[]) => unknown };

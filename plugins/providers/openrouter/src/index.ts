import type { ExtensionAPI } from "iyon:plugins";
import type { ProviderCapabilities, ProviderDefinition } from "@iyon/sdk";
import type { ReasoningLevel } from "iyon:api";
import { createOpenRouterProvider, DEFAULT_MODEL, type OpenRouterFactoryConfig } from "./provider.ts";
import { capabilitiesFromCatalog, ALL_REASONING_LEVELS, discoverModels } from "./models.ts";
import { login, logout, status } from "./auth.ts";

export const openRouterProvider: ProviderDefinition = {
  id: "openrouter",
  defaultModel: DEFAULT_MODEL,
  create: (config) => createOpenRouterProvider((config ?? {}) as OpenRouterFactoryConfig),
  capabilities: (_model: string): ProviderCapabilities => ({ reasoning: ALL_REASONING_LEVELS, tools: true, vision: true, streaming: true }),
  models: async (config) => discoverModels(config as { readonly baseUrl?: string; readonly fetch?: typeof fetch }),
  auth: { login, logout, status },
};

export function capabilitiesForCatalogModel(value: Record<string, unknown>): ProviderCapabilities { return capabilitiesFromCatalog(value); }
export function normalizeReasoningEfforts(efforts: readonly string[]): readonly ReasoningLevel[] { return efforts.filter((value): value is ReasoningLevel => ALL_REASONING_LEVELS.includes(value as ReasoningLevel)); }

export function activate(api: ExtensionAPI): void { api.providers.register(openRouterProvider); }

export { OpenRouterProvider, createOpenRouterProvider } from "./provider.ts";
export * from "./serialize.ts";
export * from "./sse.ts";

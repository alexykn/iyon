import type { CredentialStore, ProviderAuthContext, ProviderDefinition } from "@iyon/sdk";
import { MemoryCredentialStore } from "../credentials.ts";
import { ProviderSelectionError, type ModelSelection, type ProviderSelectionWarning, type ResolvedProvider } from "./types.ts";

export interface ProviderRegistryLike {
  list(): readonly { readonly value: Partial<ProviderDefinition> & Pick<ProviderDefinition, "id"> }[];
  get(id: string): (Partial<ProviderDefinition> & Pick<ProviderDefinition, "id">) | undefined;
}

export interface SelectionOptions {
  readonly registry: ProviderRegistryLike;
  readonly credentials?: CredentialStore;
  readonly config?: unknown;
  readonly env?: NodeJS.ProcessEnv;
  readonly warn?: (warning: ProviderSelectionWarning) => void;
  readonly authContext?: Omit<ProviderAuthContext, "credentials">;
}

export async function selectProvider(options: SelectionOptions): Promise<ResolvedProvider> {
  const credentials = options.credentials ?? new MemoryCredentialStore();
  const definitions = options.registry.list().map((entry) => entry.value).filter(isExecutableProvider);
  if (definitions.length === 0) throw new ProviderSelectionError("no provider registered");
  const explicit = options.env?.IYON_PROVIDER ?? process.env.IYON_PROVIDER;
  const requested = explicit === undefined ? undefined : normalizeProvider(explicit);
  const providerId = requested ?? await autoSelect(definitions, credentials, options);
  const candidate = options.registry.get(providerId);
  const definition = candidate && isExecutableProvider(candidate) ? candidate : undefined;
  if (!definition) return fallbackToMock(options, credentials, providerId, "provider is not registered");
  if (definition.id !== "mock" && definition.auth?.status) {
    const status = await safeStatus(definition, credentials, options);
    if (!status.authenticated) return fallbackToMock(options, credentials, providerId, "provider credentials are unavailable");
  }
  try {
    const modelId = options.env?.IYON_MODEL ?? process.env.IYON_MODEL ?? definition.defaultModel;
    const model = await definition.create(withCredentials(options.config, credentials, modelId));
    return { definition, model, selection: { provider: definition.id, model_id: modelId } };
  } catch (error) {
    return fallbackToMock(options, credentials, providerId, error instanceof Error ? error.message : "provider could not be created");
  }
}

export const resolveProvider = selectProvider;

function normalizeProvider(value: string): string {
  switch (value.trim().toLowerCase()) {
    case "openrouter": return "openrouter";
    case "codex":
    case "openai":
    case "openai-codex": return "openai-codex";
    case "mock": return "mock";
    default: return value.trim().toLowerCase();
  }
}

function isExecutableProvider(value: Partial<ProviderDefinition> & Pick<ProviderDefinition, "id">): value is ProviderDefinition {
  return typeof value.defaultModel === "string" && typeof value.create === "function" && typeof value.capabilities === "function";
}

async function autoSelect(definitions: readonly ProviderDefinition[], credentials: CredentialStore, options: SelectionOptions): Promise<string> {
  for (const id of ["openrouter", "openai-codex"]) {
    const definition = definitions.find((candidate) => candidate.id === id);
    if (!definition?.auth?.status) continue;
    try {
      if ((await safeStatus(definition, credentials, options)).authenticated) return id;
    } catch { /* unavailable providers are skipped during automatic detection */ }
  }
  return "mock";
}

async function safeStatus(definition: ProviderDefinition, credentials: CredentialStore, options: SelectionOptions) {
  const context: ProviderAuthContext = { ...options.authContext, credentials };
  return definition.auth!.status!(context);
}

async function fallbackToMock(options: SelectionOptions, credentials: CredentialStore, requested: string, reason: string): Promise<ResolvedProvider> {
  const candidate = options.registry.get("mock");
  const mock = candidate && isExecutableProvider(candidate) ? candidate : undefined;
  options.warn?.({ provider: requested, message: `${requested} unavailable (${reason}); falling back to mock` });
  if (!mock) throw new ProviderSelectionError("no provider registered: selected provider is unavailable and mock is not registered");
  try {
    return { definition: mock, model: await mock.create(withCredentials(options.config, credentials, mock.defaultModel)), selection: { provider: "mock", model_id: mock.defaultModel } };
  } catch (error) {
    throw new ProviderSelectionError(`no provider registered: mock could not be created (${error instanceof Error ? error.message : "unknown error"})`);
  }
}

function withCredentials(config: unknown, credentials: CredentialStore, model: string): unknown {
  if (config && typeof config === "object" && !Array.isArray(config)) return { ...(config as Record<string, unknown>), credentials, model };
  return { credentials, model };
}

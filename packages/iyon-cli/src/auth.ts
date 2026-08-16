import type { CredentialStore, ProviderAuthContext, ProviderAuthStatus, ProviderDefinition } from "@iyon/sdk";
import type { ProviderRegistry } from "@iyon/plugins";
import { MemoryCredentialStore } from "@iyon/runtime";

export type AuthOperation = "login" | "logout" | "status";
export interface AuthDependencies {
  readonly registry: ProviderRegistry;
  readonly credentials?: CredentialStore;
  readonly env?: NodeJS.ProcessEnv;
  readonly context?: Omit<ProviderAuthContext, "credentials">;
  readonly output?: (line: string) => void;
}
export interface AuthResult { readonly provider: string; readonly status?: ProviderAuthStatus; }

export async function runAuth(operation: AuthOperation, dependencies: AuthDependencies): Promise<readonly AuthResult[]> {
  const credentials = dependencies.credentials ?? new MemoryCredentialStore();
  const requested = normalizeProvider(dependencies.env?.IYON_PROVIDER ?? process.env.IYON_PROVIDER);
  const providers = dependencies.registry.list().map((entry) => entry.value).filter(isProvider);
  const selected = requested === undefined ? providers : providers.filter((provider) => provider.id === requested);
  if (selected.length === 0) throw new Error(requested === undefined ? "no provider auth contributions are registered" : `provider is not registered: ${requested}`);
  const results: AuthResult[] = [];
  for (const provider of selected) {
    const hooks = provider.auth;
    if (hooks === undefined) continue;
    const context: ProviderAuthContext = { ...dependencies.context, credentials };
    if (operation === "login") {
      if (!hooks.login) throw new Error(`provider ${provider.id} does not support auth login`);
      const status = await hooks.login(context); results.push({ provider: provider.id, status });
    } else if (operation === "logout") {
      if (!hooks.logout) throw new Error(`provider ${provider.id} does not support auth logout`);
      await hooks.logout(context); results.push({ provider: provider.id });
    } else {
      if (!hooks.status) throw new Error(`provider ${provider.id} does not support auth status`);
      const status = await hooks.status(context); results.push({ provider: provider.id, status });
    }
  }
  for (const result of results) dependencies.output?.(formatAuthResult(operation, result));
  return results;
}

function formatAuthResult(operation: AuthOperation, result: AuthResult): string {
  if (operation === "logout") return `${result.provider}: logged out`;
  const status = result.status; if (!status) return `${result.provider}: complete`;
  return `${result.provider}: ${status.authenticated ? "authenticated" : "not logged in"}`;
}
function normalizeProvider(value: string | undefined): string | undefined {
  if (value === undefined) return undefined;
  switch (value.trim().toLowerCase()) { case "codex": case "openai": case "openai-codex": return "openai-codex"; case "openrouter": return "openrouter"; case "mock": return "mock"; default: return value.trim().toLowerCase(); }
}
function isProvider(value: unknown): value is ProviderDefinition {
  return !!value && typeof value === "object" && typeof (value as ProviderDefinition).id === "string" && typeof (value as ProviderDefinition).auth === "object";
}

import type { CredentialStore, ProviderAuthContext, ProviderAuthStatus } from "@iyon/sdk";

export const CREDENTIAL_SERVICE = "iyon";
export const CREDENTIAL_ACCOUNT = "openrouter";

export async function resolveApiKey(options: { readonly apiKey?: string; readonly credentials?: CredentialStore } = {}): Promise<string | undefined> {
  if (options.apiKey?.trim()) return options.apiKey;
  const environment = process.env.OPENROUTER_API_KEY;
  if (environment?.trim()) return environment;
  return options.credentials?.get(CREDENTIAL_SERVICE, CREDENTIAL_ACCOUNT).then((value) => value?.trim() ? value : undefined);
}

export async function login(context: ProviderAuthContext): Promise<ProviderAuthStatus> {
  if (!context.prompt) throw authError("OpenRouter login requires a secret prompt");
  const key = (await context.prompt("OpenRouter API key")).trim();
  if (!key) throw authError("OpenRouter API key cannot be empty");
  await context.credentials.set(CREDENTIAL_SERVICE, CREDENTIAL_ACCOUNT, key);
  context.logger?.info?.("stored OpenRouter credentials");
  return status(context);
}

export async function logout(context: ProviderAuthContext): Promise<void> {
  await context.credentials.delete(CREDENTIAL_SERVICE, CREDENTIAL_ACCOUNT);
}

export async function status(context: ProviderAuthContext): Promise<ProviderAuthStatus> {
  const environment = process.env.OPENROUTER_API_KEY?.trim();
  const stored = await context.credentials.has(CREDENTIAL_SERVICE, CREDENTIAL_ACCOUNT);
  return { provider: "openrouter", authenticated: Boolean(environment || stored), sources: [environment ? "environment" : "", stored ? "credential-store" : ""].filter(Boolean) };
}

function authError(message: string): Error & { readonly kind: "authentication" } { return Object.assign(new Error(message), { kind: "authentication" as const }); }

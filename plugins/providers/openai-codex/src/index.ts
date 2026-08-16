import type { ExtensionAPI } from "iyon:plugins";
import type { ProviderDefinition } from "@iyon/sdk";
import { createCodexProvider, DEFAULT_BASE_URL } from "./provider.ts";
import { CODEX_CAPABILITIES, CODEX_MODELS } from "./models.ts";
import { login, logout, status } from "./auth.ts";

export const openAICodexProvider: ProviderDefinition = {
  id: "openai-codex",
  defaultModel: "gpt-5.3-codex",
  create: (config) => createCodexProvider((config ?? {}) as Parameters<typeof createCodexProvider>[0]),
  capabilities: () => CODEX_CAPABILITIES,
  models: async () => CODEX_MODELS,
  auth: { login, logout, status },
};

export function activate(api: ExtensionAPI): void { api.providers.register(openAICodexProvider); }
export { OpenAICodexProvider, createCodexProvider, DEFAULT_BASE_URL } from "./provider.ts";
export * from "./serialize.ts";

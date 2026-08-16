import type { ExtensionAPI } from "iyon:plugins";
import type { ProviderCapabilities, ProviderDefinition } from "@iyon/sdk";
import { MockProvider, type MockProviderConfig } from "./provider.ts";

const capabilities: ProviderCapabilities = { reasoning: [], tools: false, streaming: true };

export const mockProvider: ProviderDefinition = {
  id: "mock",
  defaultModel: "mock",
  create: (config) => new MockProvider((config ?? {}) as MockProviderConfig),
  capabilities: () => capabilities,
};

export function activate(api: ExtensionAPI): void {
  api.providers.register(mockProvider);
}

export { MockProvider } from "./provider.ts";

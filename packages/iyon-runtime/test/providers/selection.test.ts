import { describe, expect, test } from "bun:test";
import { createRegistries } from "@iyon/plugins";
import { MemoryCredentialStore } from "../../src/credentials.ts";
import { ProviderSelectionError } from "../../src/providers/types.ts";
import { selectProvider } from "../../src/providers/selection.ts";

const mock = { id: "mock", defaultModel: "mock", create: () => ({ stream: async function* () {} }), capabilities: () => ({ reasoning: [] }) };
const unavailable = { id: "openrouter", defaultModel: "model", create: () => { throw new Error("no key"); }, capabilities: () => ({ reasoning: [] }), auth: { status: async () => ({ provider: "openrouter", authenticated: false }) } };

describe("provider selection", () => {
  test("uses aliases and falls back visibly to registered mock", async () => {
    const registries = createRegistries();
    registries.providers.register(unavailable);
    registries.providers.register(mock);
    const warnings: string[] = [];
    const result = await selectProvider({ registry: registries.providers, credentials: new MemoryCredentialStore(), env: { IYON_PROVIDER: "openrouter" }, warn: ({ message }) => warnings.push(message) });
    expect(result.selection).toEqual({ provider: "mock", model_id: "mock" });
    expect(warnings[0]).toContain("falling back to mock");
  });

  test("fails explicitly when no provider is registered", async () => {
    const registries = createRegistries();
    await expect(selectProvider({ registry: registries.providers, env: { IYON_PROVIDER: "mock" } })).rejects.toBeInstanceOf(ProviderSelectionError);
  });
});

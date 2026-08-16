import { describe, expect, test } from "bun:test";
import { registerBundledProviders, selectProvider } from "../../src/index.ts";

describe("bundled provider bootstrap", () => {
  test("loads all bundled providers through the common extension loader", async () => {
    const loader = await registerBundledProviders();
    expect(loader.registries.providers.list().map((entry) => entry.id).sort()).toEqual(["mock", "openai-codex", "openrouter"]);
  });

  test("provider registrations can be removed without a native fallback", async () => {
    const loader = await registerBundledProviders();
    for (const entry of loader.registries.providers.list()) entry.dispose.dispose();
    expect(loader.registries.providers.list()).toHaveLength(0);
    await expect(selectProvider({ registry: loader.registries.providers, env: { IYON_PROVIDER: "mock" } })).rejects.toThrow("no provider registered");
  });
});

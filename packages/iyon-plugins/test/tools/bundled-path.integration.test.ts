import { describe, expect, test } from "bun:test";
import { registerBundledTools } from "../../src/bundled-tools.ts";

describe("bundled tool product path", () => {
  test("exposes only contribution-owned tool definitions", async () => {
    const loader = await registerBundledTools();
    const definitions = loader.registries.tools.list().map((entry) => entry.value as { id: string; execute?: unknown; renderCall?: unknown; renderResult?: unknown });
    expect(definitions).toHaveLength(7);
    expect(definitions.every((definition) => typeof definition.execute === "function" && typeof definition.renderCall === "function" && typeof definition.renderResult === "function")).toBe(true);
    expect(definitions.some((definition) => "isBuiltin" in definition)).toBe(false);
  });
});

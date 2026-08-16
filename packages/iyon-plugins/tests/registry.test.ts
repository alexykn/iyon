import { describe, expect, test } from "bun:test";
import { ToolRegistry, ProviderRegistry, AgentRegistry, AppRegistry, CommandRegistry, ShortcutRegistry } from "../src/index.ts";

describe("source-owned layered registries", () => {
  test("requires explicit replacement and restores the previous layer", () => {
    const registry = new ToolRegistry();
    registry.register({ id: "same", value: "base" });
    expect(() => registry.register({ id: "same", value: "duplicate" })).toThrow("replace: true");
    const replacement = registry.register({ id: "same", value: "replacement" }, { replace: true });
    expect(registry.lookup("same")?.value.value).toBe("replacement");
    replacement.dispose();
    expect(registry.lookup("same")?.value.value).toBe("base");
    replacement.dispose();
  });

  test("does not let a stale owner remove a newer generation", () => {
    const registry = new ToolRegistry();
    const first = registry.register({ id: "same", value: "first" });
    const second = registry.register({ id: "same", value: "second" }, { replace: true });
    first.dispose();
    expect(registry.lookup("same")?.value.value).toBe("second");
    second.dispose();
    expect(registry.lookup("same")?.value).toBeUndefined();
  });

  test("all typed facades share the same lifecycle contract", () => {
    for (const registry of [new ToolRegistry(), new ProviderRegistry(), new AgentRegistry(), new AppRegistry(), new CommandRegistry(), new ShortcutRegistry()]) {
      registry.register({ id: "one" } as never);
      expect(registry.lookup("one")?.source.packageId).toBe("direct");
    }
  });
});

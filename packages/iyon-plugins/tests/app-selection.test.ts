import { describe, expect, test } from "bun:test";
import { AppRegistry } from "../src/app-registry.ts";
import { selectApp } from "../src/app-selection.ts";

describe("app selection", () => {
  test("creates only the selected app", async () => {
    const calls: string[] = [];
    const registry = new AppRegistry();
    registry.register({ id: "first", create: () => { calls.push("first"); return {}; } });
    registry.register({ id: "replacement", create: () => { calls.push("replacement"); return {}; } });
    const selected = await selectApp(registry, { id: "replacement" });
    expect(selected.id).toBe("replacement");
    expect(calls).toEqual(["replacement"]);
  });
});

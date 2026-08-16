import { describe, expect, test } from "bun:test";
import { runBootstrap, type BootstrapStages } from "../src/bootstrap.ts";

function stages(order: string[]): BootstrapStages {
  return {
    loadConfig: async () => { order.push("config"); return {}; }, initializeNative: async () => { order.push("native"); return {}; }, initializeVirtualModules: async () => { order.push("virtual"); }, discoverPackages: async () => { order.push("discover"); return []; }, activateExtensions: async () => { order.push("activate"); return {}; }, selectProvider: async () => { order.push("provider"); return {}; }, selectAgent: async () => { order.push("agent"); return {}; }, selectApp: async () => { order.push("app"); return {}; }, runApp: async () => { order.push("run"); return "done"; }, cleanup: async () => { order.push("cleanup"); },
  };
}

describe("CLI bootstrap", () => {
  test("runs stages in order and cleans up", async () => { const order: string[] = []; const result = await runBootstrap({ type: "run" }, stages(order)); expect(order).toEqual(["config", "native", "virtual", "discover", "activate", "provider", "agent", "app", "run", "cleanup"]); expect(result.result).toBe("done"); });
  test("cleans up after a run failure", async () => { const order: string[] = []; const value = stages(order); value.runApp = async () => { order.push("run"); throw new Error("boom"); }; await expect(runBootstrap({ type: "run" }, value)).rejects.toThrow("boom"); expect(order.at(-1)).toBe("cleanup"); });
});

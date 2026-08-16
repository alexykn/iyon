import { describe, expect, test } from "bun:test";
import { installIyonVirtualModules } from "../src/virtual-modules.ts";
import { discoverPackageRoot, PackageLoader } from "@iyon/plugins";

installIyonVirtualModules();

const packageRoot = new URL("../../../plugins/agents/iyon/", import.meta.url).pathname;

describe("T9 agent registration integration", () => {
  test("bundled package uses the same loader path as external packages", async () => {
    const loader = new PackageLoader();
    const candidate = await discoverPackageRoot(packageRoot, "bundled");
    const results = await loader.load(candidate);
    expect(results[0]?.ok).toBe(true);
    expect(loader.registries.agents.get("iyon")?.id).toBe("iyon");
    await loader.unload("@iyon/agent-iyon");
  });

  test("activation rollback leaves no partial registration", async () => {
    const loader = new PackageLoader();
    const candidate = await discoverPackageRoot(packageRoot, "bundled");
    loader.registries.agents.register({ id: "iyon", create: () => ({}) });
    const results = await loader.load(candidate);
    expect(results[0]?.ok).toBe(false);
    expect(loader.registries.agents.get("iyon")?.create).toBeFunction();
  });
});

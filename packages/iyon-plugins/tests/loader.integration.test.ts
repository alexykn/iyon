import { describe, expect, test } from "bun:test";
import { installIyonVirtualModules } from "@iyon/runtime";
import { PackageLoader } from "../src/loader.ts";
import { discoverPackageRoot } from "../src/discovery.ts";

describe("plugin loader integration", () => {
  test("resolves canonical plugins after the shared installer", async () => {
    installIyonVirtualModules();
    const plugins = await import("iyon:plugins");
    expect(plugins.PackageLoader).toBeDefined();
    const candidate = await discoverPackageRoot(new URL("./fixtures/packages/register-tool", import.meta.url).pathname, "bundled");
    const loader = new PackageLoader();
    const result = await loader.loadOrThrow(candidate);
    const source = result[0]?.source;
    expect(typeof source === "object" ? source.type : undefined).toBe("local");
  });
});

import { describe, expect, test } from "bun:test";
import { installIyonVirtualModules } from "@iyon/runtime";
import { discoverPackageRoot } from "../src/discovery.ts";
import { PackageLoader } from "../src/loader.ts";

describe("extension unload", () => {
  test("disposes owned resources and exposes the prior layer", async () => {
    installIyonVirtualModules();
    const loader = new PackageLoader();
    loader.registries.tools.register({ id: "fixture.override", value: "base" });
    const candidate = await discoverPackageRoot(new URL("./fixtures/packages/override-builtin", import.meta.url).pathname, "user");
    await loader.loadOrThrow(candidate);
    expect(loader.registries.tools.lookup("fixture.override")?.value.value).toBe("extension");
    await loader.unload("fixture-override-builtin", "fixture-override-builtin");
    expect(loader.registries.tools.lookup("fixture.override")?.value.value).toBe("base");
  });
});

import { describe, expect, test } from "bun:test";
import { installIyonVirtualModules } from "@iyon/runtime";
import { discoverPackages } from "../src/discovery.ts";
import { PackageLoader } from "../src/loader.ts";

const root = new URL("./fixtures/packages/", import.meta.url).pathname;

describe("fixture extension packages", () => {
  test("loads bundled and external fixtures through one loader", async () => {
    installIyonVirtualModules();
    const candidates = await discoverPackages({
      bundled: [{ root: new URL("./fixtures/bundled", import.meta.url).pathname, manifest: { name: "fixture-bundled", version: "1.0.0", keywords: ["iyon-package"], iyon: { extensions: "./index.ts" } } }],
      user: ["register-tool", "multiple-contributions", "override-builtin", "async-activation", "npm-dependency", "scene-composer", "scene-replacement"].map((name) => ({ root: `${root}${name}` })),
      project: [{ root: `${root}activation-failure` }],
    });
    const loader = new PackageLoader();
    loader.registries.tools.register({ id: "fixture.override", value: "builtin" });
    const result = await loader.loadAll(candidates);
    expect(result.failures.some((failure) => failure.packageId === "fixture-activation-failure")).toBe(true);
    expect(result.loaded.some((loaded) => loaded.ok && loaded.packageId === "fixture-npm-dependency")).toBe(true);
    expect(loader.registries.tools.lookup("fixture.override")?.value.value).toBe("extension");
    await loader.unload("fixture-override-builtin");
    expect(loader.registries.tools.lookup("fixture.override")?.value.value).toBe("builtin");
    expect(loader.registries.tools.lookup("fixture.failure.one")).toBeUndefined();
  });
});

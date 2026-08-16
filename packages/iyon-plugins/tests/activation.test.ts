import { describe, expect, test } from "bun:test";
import { installIyonVirtualModules } from "@iyon/runtime";
import { discoverPackageRoot } from "../src/discovery.ts";
import { PackageLoader } from "../src/loader.ts";

const fixtureRoot = new URL("./fixtures/packages/", import.meta.url).pathname;

describe("transactional activation", () => {
  test("loads a fixture through the shared path and rolls back failures", async () => {
    installIyonVirtualModules();
    const loader = new PackageLoader();
    const good = await discoverPackageRoot(`${fixtureRoot}register-tool`, "user");
    const success = await loader.loadOrThrow(good);
    expect(success[0]?.ok).toBe(true);
    expect(loader.registries.tools.lookup("fixture.tool")).toBeDefined();
    await loader.unload("fixture-register-tool");
    expect(loader.registries.tools.lookup("fixture.tool")).toBeUndefined();

    const bad = await discoverPackageRoot(`${fixtureRoot}activation-failure`, "project");
    const failures = await loader.load(bad);
    expect(failures[0]?.ok).toBe(false);
    expect(loader.registries.tools.lookup("fixture.failure.one")).toBeUndefined();
  });
});

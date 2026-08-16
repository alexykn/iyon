import { describe, expect, test } from "bun:test";
import { installIyonVirtualModules } from "@iyon/runtime";
import { createAppHarness } from "@iyon/runtime/tui";
import { discoverPackageRoot, PackageLoader, selectApp } from "@iyon/plugins";
import type { IyonApp } from "../src/app.ts";

installIyonVirtualModules();

const packageRoot = new URL("../", import.meta.url).pathname;

describe("default app package", () => {
  test("loads through the ordinary app contribution path and has a native lifecycle", async () => {
    const loader = new PackageLoader();
    const candidate = await discoverPackageRoot(packageRoot, "bundled");
    const results = await loader.load(candidate);

    expect(results[0]?.ok).toBe(true);
    expect(loader.registries.apps.get("iyon")?.id).toBe("iyon");

    const harness = await createAppHarness({ width: 40, height: 12 });
    const selected = await selectApp(loader.registries.apps, {
      id: "iyon",
      context: { agent: {}, core: {}, model: { provider: "mock", modelId: "mock" }, tui: harness },
    });
    const app = selected.app as IyonApp;

    await app.start();
    expect(harness.exited()).toBe(false);
    expect(harness.screenRows()).toContain("Iyon");
    await app.stop();
    await harness.close();
    await loader.unload("@iyon/app-iyon");
  });
});

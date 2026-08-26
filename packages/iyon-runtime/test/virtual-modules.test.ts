import { describe, expect, test } from "bun:test";

import { installIyonVirtualModules } from "../src/virtual-modules.ts";

describe("S5 virtual modules", () => {
  test("load before application imports", async () => {
    installIyonVirtualModules();
    const [api, core, tui] = await Promise.all([
      import("iyon:api"),
      import("iyon:core"),
      import("iyon:tui"),
    ]);

    expect(api.apiSmoke).toBe("iyon:api/t1");
    expect(core.coreSmoke).toBe("iyon:core/t1");
    expect(tui.tuiSmoke).toBe("iyon:tui/t1");
  });
});

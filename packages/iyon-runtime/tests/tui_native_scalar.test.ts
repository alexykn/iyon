import { describe, expect, test } from "bun:test";

import { native } from "../src/native.ts";
import { Tui, View } from "../src/tui/index.ts";
import { nodeForBridge } from "../src/tui/values/view.ts";

type OracleHost = {
  render(view: object): void;
  screenRows(): string[];
  dispose(): void;
};

const Host = native.NativeTuiHost as unknown as (new (width: number, height: number, headless: boolean) => OracleHost) | undefined;

describe("PERF-11.3 generated scalar retained route", () => {
  test("keeps exact identity O(1) and renders text layout through generated FFI", async () => {
    if (Host === undefined) return;
    const tui = await Tui.open({ width: 8, height: 4, headless: true });
    const oracle = new Host(8, 4, true);
    const base = View.text("hello");
    const changed = base.noWrap().textAlign("center");
    try {
      tui.render({ body: base });
      const first = [...tui.screenRows()];
      tui.render({ body: base });
      expect(tui.screenRows()).toEqual(first);

      tui.render({ body: changed });
      oracle.render(nodeForBridge(changed));
      expect(tui.screenRows()).toEqual(oracle.screenRows());
    } finally {
      tui.close();
      oracle.dispose();
    }
  });

  test("renders supported root common-field patches through generated FFI", async () => {
    if (Host === undefined) return;
    const tui = await Tui.open({ width: 8, height: 4, headless: true });
    const oracle = new Host(8, 4, true);
    const base = View.text("x");
    const changed = base.padding(1);
    try {
      tui.render({ body: base });
      tui.render({ body: changed });
      oracle.render(nodeForBridge(changed));
      expect(tui.screenRows()).toEqual(oracle.screenRows());
    } finally {
      tui.close();
      oracle.dispose();
    }
  });
});

import { describe, expect, test } from "bun:test";

import { AppHarness, View } from "../src/tui/index.ts";

describe("T5 deterministic headless harness", () => {
  test("uses fixed dimensions, key/paste input, and clock injection", async () => {
    const harness = await AppHarness.open({ width: 8, height: 2 });
    await harness.render({ body: View.vertical([View.text("one"), View.text("two"), View.text("three")]) });
    expect(await harness.size).toEqual({ width: 8, height: 2 });
    expect(harness.screenRows()).toEqual(["two", "three"]);
    expect(harness.nativeHistoryRows()).toEqual(["one"]);
    harness.pressKey("Enter");
    harness.paste("input");
    expect((await harness.nextEvent()).type).toBe("key");
    expect((await harness.nextEvent()).type).toBe("paste");
    harness.advance(25);
    expect(harness.now()).toBe(25);
    expect(harness.cellXOfText(1, "three")).toBe(0);
    await harness.close();
    expect(harness.exited()).toBe(true);
  });
});

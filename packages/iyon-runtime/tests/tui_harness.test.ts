import { describe, expect, test } from "bun:test";

import { AppHarness, TextStream, View } from "../src/tui/index.ts";

describe("native headless harness", () => {
  test("uses native snapshots and dispatches input through the mounted host", async () => {
    const harness = await AppHarness.open({ width: 20, height: 4 });
    const input = harness.createTextInput({ multiline: true });
    const history = harness.createHistory();
    await history.push(View.text("native history"));
    await harness.render({ body: View.vertical([View.component(input), View.text("footer")]), history });
    expect(harness.screenRows().at(-1)?.startsWith("footer")).toBe(true);
    expect(harness.nativeHistoryRows().some((row) => row.includes("native history"))).toBe(true);

    harness.bindKey("Escape", "escape");
    harness.pressKey("a");
    expect(await input.text()).toBe("a");
    harness.route(await input.submitted(), "submit");
    harness.pressKey("Enter");
    await expect(harness.nextAction()).resolves.toEqual({ actionId: "submit", payload: "a" });

    harness.advance(25);
    expect(harness.now()).toBe(25);
    await harness.close();
    expect(harness.exited()).toBe(true);
  });

  test("streams update the mounted native History", async () => {
    const harness = await AppHarness.open({ width: 24, height: 6 });
    const history = harness.createHistory();
    const stream = new TextStream();
    await harness.render({ body: View.text("footer"), history });
    await history.pushStream(stream);
    await stream.update("assistant");
    expect(harness.screenRows().some((row) => row.includes("assistant"))).toBe(true);
    await stream.seal();
    await expect(stream.update("late")).rejects.toThrow();
    await harness.close();
  });

  test("ticks the native working component and renders its queue", async () => {
    const harness = await AppHarness.open({ width: 80, height: 6 });
    const working = harness.createWorking();
    await working.setActive(true);
    await harness.render({ body: View.component(working) });
    expect(harness.screenRows().some((row) => row.includes("⠋⣠ Working"))).toBe(true);
    harness.advance(80);
    expect(harness.screenRows().some((row) => row.includes("⢁⡴ Working"))).toBe(true);
    await working.setPending(["hello   world", "second"]);
    await harness.render({ body: View.component(working) });
    expect(harness.screenRows().some((row) => row.includes("waiting") && row.includes("Queue: hello world") && row.includes("+ 1 more"))).toBe(true);
    await working.dispose();
    await harness.close();
  });

  test("observes native styles and terminal-cell Unicode positions", async () => {
    const harness = await AppHarness.open({ width: 12, height: 2 });
    await harness.render({ body: View.text("a🌍b").bold().noWrap() });
    expect(harness.cellXOfText(1, "🌍")).toBe(1);
    expect(harness.cellXOfText(1, "b")).toBe(3);
    expect(harness.styleAt(1, 0).bold).toBe(true);
    await harness.close();
    expect(harness.exited()).toBe(true);
  });
});

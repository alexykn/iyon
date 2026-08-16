import { describe, expect, test } from "bun:test";

import { Scene, Tui, View, keyEvent, pasteEvent } from "../src/tui/index.ts";

describe("T5 TS-owned TUI runtime", () => {
  test("renders a generic Scene and routes owned events", async () => {
    const tui = await Tui.open({ width: 20, height: 4, headless: true });
    expect(await tui.size).toEqual({ width: 20, height: 4 });
    await tui.render(new Scene(View.text("hello")));
    tui.enqueue(keyEvent("a"));
    tui.enqueue(pasteEvent("paste"));
    expect(await tui.nextEvent()).toEqual({ type: "key", key: "a", modifiers: undefined });
    expect(await tui.nextEvent()).toEqual({ type: "paste", text: "paste" });
    await tui.close();
    expect((await tui.nextEvent()).type).toBe("terminate");
  });

  test("cancellation rejects pending input and close is idempotent", async () => {
    const tui = await Tui.open({ headless: true });
    const controller = new AbortController();
    const waiting = tui.nextEvent(controller.signal);
    controller.abort();
    await expect(waiting).rejects.toMatchObject({ category: "cancelled" });
    await tui.close();
    await tui.close();
  });
});

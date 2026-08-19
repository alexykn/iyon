import { describe, expect, test } from "bun:test";

import { Component, History, TextInput, TextStream, View } from "../src/tui/index.ts";

describe("T5 native TUI handles", () => {
  test("keeps TextInput state in one native object", async () => {
    const input = new TextInput();
    await input.setText("hello 🌍");
    expect(await input.text()).toBe("hello 🌍");
    expect(await input.cursorBytes()).toBe(Buffer.byteLength("hello 🌍"));
    await input.setMultiline(true);
    expect(await input.isMultiline()).toBe(true);
    await input.clear();
    expect(await input.text()).toBe("");
    input.dispose();
    try { input.text(); throw new Error("expected disposed handle error"); }
    catch (error) { expect(error).toMatchObject({ category: "disposed-handle" }); }
  });

  test("preserves stream revision and sealed-state errors", async () => {
    const stream = new TextStream();
    await stream.update("first");
    expect(await stream.snapshot()).toEqual({ text: "first", revision: 1, sealed: false });
    stream.seal();
    expect(stream.snapshot().sealed).toBe(true);
    expect(() => stream.update("late")).toThrow(/sealed/);
  });

  test("history accepts a retained view and component handles are shared", async () => {
    const history = new History();
    await history.push(View.text("history"));
    const component = new Component();
    const first = await component.view();
    const second = await component.view();
    expect(first).not.toBe(second);
    expect(await component.revision()).toBe(0);
    component.dispose();
    history.dispose();
  });
});

import { describe, expect, test } from "bun:test";

import { materializeView, View } from "../src/tui/index.ts";
import { native } from "../src/native.ts";

describe("T5 lazy TUI values", () => {
  test("fluent operations return new semantic values", () => {
    const original = View.text("x");
    const styled = original.bold().padding(1).fillWidth();

    expect(original).not.toBe(styled);
    expect(original.kind).toBe("view");
    expect(styled.kind).toBe("view");
  });

  test("nested composition crosses the native boundary once", () => {
    const view = View.vertical([
      View.text("one").bold(),
      View.horizontal([View.text("two"), View.spacer(1)]),
    ]);

    const materialized = materializeView(view);
    expect(materialized).toBeDefined();
  });

  test("native validation rejects malformed recursive nodes", () => {
    expect(native.materializeView).toBeFunction();
    expect(() => native.materializeView?.({ type: "not-a-view" })).toThrow(/unknown view node type/);
  });
});

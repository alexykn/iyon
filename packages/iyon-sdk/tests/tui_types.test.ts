import { describe, expect, test } from "bun:test";
import type { History, Scene, View } from "../src/tui/index.d.ts";

describe("T5 SDK TUI declarations", () => {
  test("exposes the bindable Scene contract", () => {
    const body = { kind: "view" } as View;
    const scene: Scene = { body };
    const history = undefined as History | undefined;
    expect(scene.body).toBe(body);
    expect(history).toBeUndefined();
  });
});

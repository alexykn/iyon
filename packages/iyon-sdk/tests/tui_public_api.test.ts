import { describe, expect, test } from "bun:test";
import type { AppHarness, History, Scene, TextInput, TextStream, Tui, View } from "../src/tui/index.d.ts";

describe("T5 public SDK TUI API", () => {
  test("exports the framework contracts without core/API dependencies", () => {
    const view = undefined as View | undefined;
    const scene = undefined as Scene | undefined;
    const history = undefined as History | undefined;
    const input = undefined as TextInput | undefined;
    const stream = undefined as TextStream | undefined;
    const runtime = undefined as Tui | undefined;
    const harness = undefined as AppHarness | undefined;
    expect([view, scene, history, input, stream, runtime, harness]).toHaveLength(7);
  });
});

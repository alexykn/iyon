import { describe, expect, test } from "bun:test";
import { installIyonVirtualModules } from "@iyon/runtime";
import { AssistantStreamBuffer, NativeAssistantStream } from "../src/streaming.ts";

installIyonVirtualModules();

describe("assistant streaming", () => {
  test("coalesces adjacent semantic segments and seals", () => {
    const stream = new AssistantStreamBuffer(); stream.append("text", "a"); stream.append("text", "b"); stream.append("thinking", "c");
    expect(stream.snapshot()).toEqual([{ kind: "text", text: "ab" }, { kind: "thinking", text: "c" }]);
    stream.seal(); expect(stream.isSealed()).toBe(true); expect(() => stream.append("text", "d")).toThrow();
  });
  test("uses the native stream handle", async () => {
    const stream = new NativeAssistantStream(); await stream.append("text", "hello");
    expect((await stream.snapshot()).text).toBe("hello"); await stream.seal(); await stream.dispose();
  });
});

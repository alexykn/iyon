import { describe, expect, test } from "bun:test";
import { installIyonVirtualModules } from "@iyon/runtime";
import type { ModelApi, ModelRequest, ModelStreamEvent } from "iyon:api";
import { runProviderTurn } from "../src/turn.ts";
import { SessionDriver } from "./support/session-driver.ts";
import { emptyRequest, textTurn } from "./support/fixtures.ts";

installIyonVirtualModules();

describe("bundled agent model turns", () => {
  test("turn_assembles_text_and_reasoning_deltas", async () => {
    const driver = await SessionDriver.create(301);
    const model = scriptedModel([
      { type: "started" },
      { type: "thinkingDelta", contentIndex: 0, delta: "think" },
      { type: "textDelta", contentIndex: 1, delta: "answer" },
      { type: "usage", usage: { inputTokens: 1, outputTokens: 2, cacheReadTokens: 3, cacheWriteTokens: 4 } },
      { type: "done", stopReason: "stop" },
    ]);
    const result = await runProviderTurn({ session: driver.session, model }, emptyRequest());
    expect(result.assistantMessage.role).toBe("assistant");
    if (result.assistantMessage.role === "assistant") expect(result.assistantMessage.content).toEqual([{ type: "thinking", text: "think" }, { type: "text", text: "answer" }]);
    expect(result.stopReason).toBe("stop");
    driver.close();
  });

  test("turn_assembles_multiple_tool_calls_in_order", async () => {
    const driver = await SessionDriver.create(302);
    const events: ModelStreamEvent[] = [
      { type: "toolCallStart", contentIndex: 0, id: "a", name: "first" },
      { type: "toolCallEnd", contentIndex: 0, id: "a", name: "first", arguments: {} },
      { type: "toolCallStart", contentIndex: 1, id: "b", name: "second" },
      { type: "toolCallEnd", contentIndex: 1, id: "b", name: "second", arguments: { value: 2 } },
      { type: "done", stopReason: "toolUse" },
    ];
    const result = await runProviderTurn({ session: driver.session, model: scriptedModel(events) }, emptyRequest());
    expect(result.toolCalls.map((call) => call.name)).toEqual(["first", "second"]);
    driver.close();
  });

  test("turn_returns_partial_assistant_on_abort", async () => {
    const driver = await SessionDriver.create(303);
    const controller = new AbortController();
    const model: ModelApi = {
      async *stream(_request, options) {
        yield { type: "textDelta", contentIndex: 0, delta: "partial" };
        await waitForAbort(options?.signal);
        yield { type: "done", stopReason: "aborted" };
      },
    };
    const pending = runProviderTurn({ session: driver.session, model, signal: controller.signal }, emptyRequest());
    await Bun.sleep(1);
    controller.abort();
    const result = await pending;
    expect(result.cancelled).toBe(true);
    expect(result.assistantMessage.role).toBe("assistant");
    if (result.assistantMessage.role === "assistant") expect(result.assistantMessage.content).toEqual([{ type: "text", text: "partial" }]);
    driver.close();
  });

  test("turn_preserves_length_stop", async () => {
    const driver = await SessionDriver.create(304);
    const result = await runProviderTurn({ session: driver.session, model: scriptedModel(textTurn("cut", "length")) }, emptyRequest());
    expect(result.stopReason).toBe("length");
    driver.close();
  });

  test("turn_rejects_provider_stream_error", async () => {
    const driver = await SessionDriver.create(305);
    const model: ModelApi = { stream: async function* () { throw new Error("network down"); } };
    await expect(runProviderTurn({ session: driver.session, model }, emptyRequest())).rejects.toThrow("provider stream failed: network down");
    driver.close();
  });

  test("turn_reports_invalid_tool_call_without_dropping_it", async () => {
    const driver = await SessionDriver.create(306);
    const result = await runProviderTurn({
      session: driver.session,
      model: scriptedModel([
        { type: "toolCallStart", contentIndex: 3, id: "invalid" },
        { type: "done", stopReason: "toolUse" },
      ]),
    }, emptyRequest());
    expect(result.invalidToolCalls).toEqual([{ contentIndex: 3, id: "invalid", argumentsText: "", reason: "tool call stream ended before toolCallEnd" }]);
    driver.close();
  });
});

function scriptedModel(events: readonly ModelStreamEvent[]): ModelApi {
  return { stream: async function* (_request: ModelRequest) { for (const event of events) yield event; } };
}

async function waitForAbort(signal?: AbortSignal): Promise<void> {
  if (signal?.aborted) return;
  await new Promise<void>((resolve) => signal?.addEventListener("abort", () => resolve(), { once: true }));
}

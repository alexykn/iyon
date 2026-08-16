import { describe, expect, test } from "bun:test";
import { installIyonVirtualModules } from "@iyon/runtime";
import type { ModelApi, ModelRequest, ModelStreamEvent } from "iyon:api";
import type { ToolDefinition, ToolResult } from "@iyon/sdk";
import { IyonAgent, type AgentContext } from "../src/agent.ts";
import { SessionDriver } from "./support/session-driver.ts";
import { textTurn } from "./support/fixtures.ts";

installIyonVirtualModules();

class TestSteeringQueue {
  readonly items: string[] = [];
  drain(): readonly string[] { return this.items.splice(0); }
}

describe("bundled Iyon agent", () => {
  test("run_composes_request_turn_and_tools", async () => {
    const driver = await SessionDriver.create(501);
    driver.session.appendMessage({ role: "user", content: [{ type: "text", text: "read" }] });
    driver.enqueue(toolTurn("read"), textTurn("done"));
    await new IyonAgent(contextFor(driver, registry(tool("read")))).run();
    expect(driver.provider.requests).toHaveLength(2);
    expect(driver.provider.requests[1]?.messages.at(-1)?.role).toBe("toolResult");
    driver.close();
  });

  test("steering_is_drained_in_arrival_order", async () => {
    const driver = await SessionDriver.create(502);
    const steering = new TestSteeringQueue();
    steering.items.push("one", "two");
    driver.enqueue(textTurn("first"), textTurn("second"));
    await new IyonAgent(contextFor(driver, undefined, steering)).run();
    expect(driver.provider.requests[0]?.messages.slice(-2).map((message) => message.content[0])).toEqual([
      { type: "text", text: "one" },
      { type: "text", text: "two" },
    ]);
    driver.close();
  });

  test("steering_does_not_abort_inflight_response", async () => {
    const driver = await SessionDriver.create(503);
    driver.session.appendMessage({ role: "user", content: [{ type: "text", text: "initial" }] });
    const steering = new TestSteeringQueue();
    let calls = 0;
    const model: ModelApi = {
      stream: async function* () {
        calls += 1;
        yield { type: "textDelta", contentIndex: 0, delta: calls === 1 ? "first" : "second" };
        if (calls === 1) {
          setTimeout(() => steering.items.push("during"), 0);
          await Bun.sleep(2);
        }
        yield { type: "done", stopReason: "stop" };
      },
    };
    await new IyonAgent(contextFor(driver, undefined, steering, model)).run();
    expect(calls).toBe(2);
    expect(driver.provider.requests).toHaveLength(0);
    expect(driver.session.snapshot().entries.some((entry) => entry.kind === "message" && entry.role === "user" && entry.content[0]?.type === "text" && entry.content[0].text === "during")).toBe(true);
    driver.close();
  });

  test("stop_with_pending_steering_continues", async () => {
    const driver = await SessionDriver.create(504);
    driver.session.appendMessage({ role: "user", content: [{ type: "text", text: "initial" }] });
    const steering = new TestSteeringQueue();
    let calls = 0;
    const model = modelThatAddsSteer(steering, () => { calls += 1; });
    await new IyonAgent(contextFor(driver, undefined, steering, model)).run();
    expect(calls).toBe(2);
    expect(driver.session.snapshot().entries.some((entry) => entry.kind === "message" && entry.role === "user" && entry.content[0]?.type === "text" && entry.content[0].text === "after-stop")).toBe(true);
    driver.close();
  });

  test("cancel_preserves_partial_history", async () => {
    const driver = await SessionDriver.create(505);
    driver.session.appendMessage({ role: "user", content: [{ type: "text", text: "cancel" }] });
    const controller = new AbortController();
    const model: ModelApi = {
      stream: async function* (_request, options) {
        yield { type: "textDelta", contentIndex: 0, delta: "partial" };
        await waitForAbort(options?.signal);
        yield { type: "done", stopReason: "aborted" };
      },
    };
    const pending = new IyonAgent(contextFor(driver, undefined, undefined, model, controller.signal)).run();
    await Bun.sleep(1);
    controller.abort();
    await pending;
    expect(driver.session.snapshot().entries.some((entry) => entry.kind === "message" && entry.role === "assistant" && entry.content[0]?.type === "text" && entry.content[0].text === "partial")).toBe(true);
    driver.close();
  });

  test("follow_up_reuses_kernel_history", async () => {
    const driver = await SessionDriver.create(506);
    driver.session.appendMessage({ role: "user", content: [{ type: "text", text: "first" }] });
    driver.enqueue(textTurn("answer"));
    await new IyonAgent(contextFor(driver)).run();
    driver.session.appendMessage({ role: "user", content: [{ type: "text", text: "follow up" }] });
    driver.enqueue(textTurn("second"));
    await new IyonAgent(contextFor(driver)).run();
    expect(driver.provider.requests[1]?.messages.map((message) => message.role)).toEqual(["user", "assistant", "user"]);
    driver.close();
  });

  test("tool_results_feed_the_next_request", async () => {
    const driver = await SessionDriver.create(507);
    driver.session.appendMessage({ role: "user", content: [{ type: "text", text: "use tool" }] });
    driver.enqueue(toolTurn("echo"), textTurn("finished"));
    await new IyonAgent(contextFor(driver, registry(tool("echo")))).run();
    expect(driver.provider.requests[1]?.messages.at(-1)?.role).toBe("toolResult");
    driver.close();
  });

  test("run_stops_on_abort", async () => {
    const driver = await SessionDriver.create(508);
    const controller = new AbortController();
    controller.abort();
    await new IyonAgent(contextFor(driver, undefined, undefined, driver.provider, controller.signal)).run();
    expect(driver.provider.requests).toHaveLength(0);
    driver.close();
  });
});

function contextFor(driver: SessionDriver, tools?: { list(): readonly { value: unknown }[] }, steering?: TestSteeringQueue, model: ModelApi = driver.provider, signal = new AbortController().signal): AgentContext {
  return { session: driver.session, model, signal, ...(tools ? { tools } : {}), ...(steering ? { steering } : {}) };
}

function registry(...tools: ToolDefinition[]) { return { list: () => tools.map((value) => ({ value })) }; }

function tool(name: string): ToolDefinition {
  return { name, description: name, inputSchema: {}, execute: async (): Promise<ToolResult> => ({ content: [{ type: "text", text: "ok" }], details: {}, isError: false }), renderCall: () => undefined as never, renderResult: () => undefined as never };
}

function toolTurn(name: string): readonly ModelStreamEvent[] {
  return [
    { type: "toolCallStart", contentIndex: 0, id: "call-1", name },
    { type: "toolCallEnd", contentIndex: 0, id: "call-1", name, arguments: {} },
    { type: "done", stopReason: "toolUse" },
  ];
}

function modelThatAddsSteer(steering: TestSteeringQueue, onCall: () => void): ModelApi {
  let calls = 0;
  return { stream: async function* () { calls += 1; onCall(); yield { type: "textDelta", contentIndex: 0, delta: "response" }; if (calls === 1) steering.items.push("after-stop"); yield { type: "done", stopReason: "stop" }; } };
}

async function waitForAbort(signal?: AbortSignal): Promise<void> {
  if (signal?.aborted) return;
  await new Promise<void>((resolve) => signal?.addEventListener("abort", () => resolve(), { once: true }));
}

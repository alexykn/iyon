import { describe, expect, test } from "bun:test";
import { installIyonVirtualModules } from "@iyon/runtime";
import type { ToolDefinition, ToolResult } from "@iyon/sdk";
import { classifyStopReason } from "../src/stop.ts";
import { executeRequestedTools, MAX_TOOL_CALLS_PER_MODEL_TURN } from "../src/tools.ts";
import type { AgentModelTurnResult } from "../src/turn.ts";
import { SessionDriver } from "./support/session-driver.ts";

installIyonVirtualModules();

describe("bundled agent stop and tool policy", () => {
  test("tool_use_with_calls_continues", () => expect(classifyStopReason("toolUse", true)).toBe("executeTools"));
  test("stop_without_calls_finishes", () => expect(classifyStopReason("stop", false)).toBe("finish"));
  test("length_without_calls_finishes", () => expect(classifyStopReason("length", false)).toBe("finish"));
  test("tool_use_without_calls_fails", () => expect(() => classifyStopReason("toolUse", false)).toThrow());
  test("stop_with_calls_fails", () => expect(() => classifyStopReason("stop", true)).toThrow());

  test("sixteen_tool_calls_are_allowed", async () => {
    const driver = await SessionDriver.create(401);
    let executions = 0;
    const tool = testTool("allowed", async () => { executions += 1; return ok(); });
    const result = await executeRequestedTools({ session: driver.session, tools: registry(tool), turnId: 1 as never, messageId: 2 as never, signal: new AbortController().signal }, resultWithCalls(16, "allowed"));
    expect(result.completed).toBe(true);
    expect(executions).toBe(16);
    expect(MAX_TOOL_CALLS_PER_MODEL_TURN).toBe(16);
    driver.close();
  });

  test("seventeenth_tool_call_is_rejected_before_execution", async () => {
    const driver = await SessionDriver.create(402);
    let executions = 0;
    await expect(executeRequestedTools({ session: driver.session, tools: registry(testTool("blocked", async () => { executions += 1; return ok(); })), turnId: 1 as never, messageId: 2 as never }, resultWithCalls(17, "blocked"))).rejects.toThrow("maximum is 16");
    expect(executions).toBe(0);
    driver.close();
  });

  test("tool_calls_execute_in_provider_order", async () => {
    const driver = await SessionDriver.create(403);
    const order: string[] = [];
    const first = testTool("first", async () => { order.push("first:start"); await Bun.sleep(2); order.push("first:end"); return ok(); });
    const second = testTool("second", async () => { order.push("second:start"); order.push("second:end"); return ok(); });
    await executeRequestedTools({ session: driver.session, tools: registry(first, second), turnId: 1 as never, messageId: 2 as never }, resultWithCalls(2, "first", "second"));
    expect(order).toEqual(["first:start", "first:end", "second:start", "second:end"]);
    driver.close();
  });

  test("inactive_tool_call_is_reported_as_invalid", async () => {
    const driver = await SessionDriver.create(404);
    const summary = await executeRequestedTools({ session: driver.session, tools: registry(), turnId: 1 as never, messageId: 2 as never }, resultWithCalls(1, "missing"));
    expect(summary.results[0]?.isError).toBe(true);
    expect(driver.snapshot().entries.some((entry) => entry.kind === "message" && entry.role === "toolResult")).toBe(true);
    driver.close();
  });

  test("approval_and_cancellation_use_public_execution", async () => {
    const driver = await SessionDriver.create(405);
    let approved = false;
    await executeRequestedTools({ session: driver.session, tools: registry(testTool("ask", async () => { approved = true; return ok(); }, "alwaysAsk")), approval: async () => false, turnId: 1 as never, messageId: 2 as never }, resultWithCalls(1, "ask"));
    expect(approved).toBe(false);
    const controller = new AbortController();
    controller.abort();
    const cancelled = await executeRequestedTools({ session: driver.session, tools: registry(testTool("cancel", async () => { approved = true; return ok(); })), signal: controller.signal, turnId: 1 as never, messageId: 2 as never }, resultWithCalls(1, "cancel"));
    expect(cancelled.completed).toBe(false);
    expect(approved).toBe(false);
    driver.close();
  });

  test("tool_result_is_committed_by_the_kernel", async () => {
    const driver = await SessionDriver.create(406);
    await executeRequestedTools({ session: driver.session, tools: registry(testTool("commit", async () => ok())), turnId: 1 as never, messageId: 2 as never }, resultWithCalls(1, "commit"));
    expect(driver.snapshot().entries.filter((entry) => entry.kind === "message" && entry.role === "toolResult")).toHaveLength(1);
    driver.close();
  });
});

function resultWithCalls(count: number, ...names: string[]): AgentModelTurnResult {
  return {
    turnId: 1 as never,
    assistantMessage: { kind: "message", role: "assistant", id: 2 as never, content: [], timestamp: "now" },
    toolCalls: Array.from({ length: count }, (_, index) => ({ id: `call-${index}` as never, name: names[index % names.length] ?? "tool", arguments: {} })),
    invalidToolCalls: [],
    stopReason: "toolUse",
    cancelled: false,
  };
}

function registry(...tools: ToolDefinition[]) {
  return { list: () => tools.map((value) => ({ value })) };
}

function testTool(name: string, execute: ToolDefinition["execute"], approval?: "alwaysAsk"): ToolDefinition {
  return { name, description: name, inputSchema: {}, ...(approval ? { approval } : {}), execute, renderCall: () => undefined as never, renderResult: () => undefined as never };
}

function ok(): ToolResult { return { content: [{ type: "text", text: "ok" }], details: {}, isError: false }; }

import { describe, expect, test } from "bun:test";
import { installIyonVirtualModules } from "@iyon/runtime";
import type { SessionEntry, ToolDefinition } from "@iyon/sdk";
import { SessionDriver } from "./support/session-driver.ts";
import { buildModelRequest } from "../src/request.ts";
import { DEFAULT_SYSTEM_PROMPT, buildSystemPrompt } from "../src/prompt.ts";

installIyonVirtualModules();

const tool = (name: string): ToolDefinition => ({
  name,
  description: `${name} description`,
  inputSchema: { type: "object" },
  execute: async () => ({ content: [{ type: "text", text: "ok" }], details: {}, isError: false }),
  renderCall: () => undefined as never,
  renderResult: () => undefined as never,
});

describe("bundled agent request policy", () => {
  test("default_prompt_is_stable", () => {
    expect(DEFAULT_SYSTEM_PROMPT).toBe("");
    expect(buildSystemPrompt()).toBeUndefined();
  });

  test("empty_prompt_is_omitted", async () => {
    const driver = await SessionDriver.create(201);
    expect(buildModelRequest({ session: driver.session, systemPrompt: "  " })).not.toHaveProperty("systemPrompt");
    driver.close();
  });

  test("request_omits_status_messages", async () => {
    const driver = await SessionDriver.create(202);
    appendMixedHistory(driver.session);
    const request = buildModelRequest({ session: driver.session, systemPrompt: "system" });
    expect(request.messages.map((message) => message.role)).toEqual(["user", "assistant", "toolResult"]);
    driver.close();
  });

  test("request_preserves_tool_results_and_metadata", async () => {
    const driver = await SessionDriver.create(203);
    appendMixedHistory(driver.session);
    const request = buildModelRequest({ session: driver.session, metadata: { userId: "user-1" } });
    expect(request.metadata).toEqual({ sessionId: "203", userId: "user-1" });
    expect(request.messages[2]).toEqual({ role: "toolResult", toolCallId: "call-1", toolName: "read", content: [{ type: "text", text: "result" }], isError: false });
    driver.close();
  });

  test("request_exposes_active_tools_only", async () => {
    const driver = await SessionDriver.create(204);
    const registry = { list: () => [{ value: tool("zeta") }, { value: tool("alpha") }, { value: { id: "inactive" } }] };
    const request = buildModelRequest({ session: driver.session, tools: registry, activeToolNames: ["zeta", "alpha"] });
    expect(request.tools.map(({ name }) => name)).toEqual(["alpha", "zeta"]);
    driver.close();
  });

  test("request_uses_session_reasoning_effort", async () => {
    const driver = await SessionDriver.create(205);
    expect(buildModelRequest({ session: driver.session, reasoningEffort: "high" }).params).toEqual({ reasoning: "high" });
    driver.close();
  });

  test("context_selection_preserves_canonical_order", async () => {
    const driver = await SessionDriver.create(206);
    appendMixedHistory(driver.session);
    const request = buildModelRequest({ session: driver.session });
    expect(request.messages.map((message) => message.content[0])).toEqual([
      { type: "text", text: "user" },
      { type: "text", text: "assistant" },
      { type: "text", text: "result" },
    ]);
    driver.close();
  });
});

function appendMixedHistory(session: { appendEntry(entry: SessionEntry): void }): void {
  session.appendEntry({ kind: "message", role: "user", id: 1 as never, content: [{ type: "text", text: "user" }], timestamp: "1" });
  session.appendEntry({ kind: "message", role: "status", id: 2 as never, text: "status", content: [], timestamp: "2" } as unknown as SessionEntry);
  session.appendEntry({ kind: "message", role: "assistant", id: 3 as never, content: [{ type: "text", text: "assistant" }], timestamp: "3" });
  session.appendEntry({ kind: "message", role: "toolResult", id: 4 as never, toolCallId: "call-1" as never, toolName: "read", content: [{ type: "text", text: "result" }], details: {}, isError: false, timestamp: "4" });
  session.appendEntry({ kind: "custom", namespace: "test", data: { ignored: true } });
}

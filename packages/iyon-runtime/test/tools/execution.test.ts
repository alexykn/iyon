import { describe, expect, test } from "bun:test";
import { defineTool } from "@iyon/sdk";
import { KernelSession } from "../../src/modules/core.ts";
import { executeTool } from "../../src/tools/execution.ts";

const tool = defineTool({
  name: "fixture",
  description: "fixture",
  inputSchema: { type: "object" },
  execute: async (context) => {
    await context.update({ type: "text", text: "working" });
    return { content: [{ type: "text", text: "done" }], details: { kept: true }, isError: false };
  },
  renderCall: () => ({}) as never,
  renderResult: () => ({}) as never,
});

function request(toolName = "fixture") {
  return { sessionId: 7 as never, turnId: 1 as never, messageId: 2 as never, toolCallId: "call-1" as never, toolName, arguments: {} };
}

describe("generic tool lifecycle", () => {
  test("runs native lifecycle, hooks, updates, and preserves result details", async () => {
    const session = new KernelSession({ id: 7 });
    const updates: string[] = [];
    const result = await executeTool(session, tool, request(), {
      updates: { send: async (update) => { if (update.type === "text") updates.push(update.text); } },
      hooks: {
        before: async () => ({ changed: true }),
        after: async (_context, value) => ({ ...value, details: { ...value.details as Record<string, unknown>, after: true } }),
      },
    });

    expect(updates).toEqual(["working"]);
    expect(result.result).toMatchObject({ isError: false, details: { kept: true, after: true } });
    expect(result.execution.state()).toBe("finished");
    expect(result.execution.events().map((event) => event.state)).toEqual(["preparing", "prepared", "running", "finished"]);
    session.close();
  });

  test("turns hook and execution failures into one native failure", async () => {
    const session = new KernelSession({ id: 8 });
    await expect(executeTool(session, tool, request(), { hooks: { before: async () => { throw new Error("blocked"); } } })).rejects.toThrow("blocked");
    const execution = session.prepareToolExecution(request());
    execution.prepared({});
    expect(execution.state()).toBe("prepared");
    session.close();
  });

  test("unknown tools receive a canonical error result", async () => {
    const session = new KernelSession({ id: 9 });
    const result = await executeTool(session, undefined, request("weather"));
    expect(result.result).toMatchObject({ isError: true, toolName: "weather" });
    expect(result.execution.state()).toBe("finished");
    session.close();
  });

  test("aborted calls cancel before execution", async () => {
    const session = new KernelSession({ id: 10 });
    const controller = new AbortController();
    controller.abort();
    await expect(executeTool(session, tool, request(), { signal: controller.signal })).rejects.toMatchObject({ code: "ION_CANCELLED" });
    session.close();
  });

  test("approval_happens_before_running", async () => {
    const session = new KernelSession({ id: 11 });
    const lifecycle: string[] = [];
    let executed = false;
    const approvalTool = defineTool({
      name: "approval-fixture",
      description: "approval-fixture",
      inputSchema: { type: "object" },
      approval: "alwaysAsk",
      execute: async () => {
        executed = true;
        return { content: [{ type: "text", text: "should not run" }], details: {}, isError: false };
      },
      renderCall: () => ({}) as never,
      renderResult: () => ({}) as never,
    });
    const result = await executeTool(session, approvalTool, request("approval-fixture"), {
      approval: async () => {
        lifecycle.push("approval");
        return false;
      },
    });

    lifecycle.unshift(...result.execution.events().map((event) => event.state));
    expect(lifecycle).toEqual(["preparing", "prepared", "pendingApproval", "failed", "approval"]);
    expect(executed).toBe(false);
    session.close();
  });

  test("unresolved_approval_never_executes_tool", async () => {
    const session = new KernelSession({ id: 12 });
    let executed = false;
    const approvalTool = defineTool({
      name: "approval-unresolved",
      description: "approval-unresolved",
      inputSchema: { type: "object" },
      approval: "alwaysAsk",
      execute: async () => {
        executed = true;
        return { content: [{ type: "text", text: "should not run" }], details: {}, isError: false };
      },
      renderCall: () => ({}) as never,
      renderResult: () => ({}) as never,
    });
    let resolveApproval: (approved: boolean) => void = () => undefined;
    const approval = new Promise<boolean>((resolve) => { resolveApproval = resolve; });
    const operation = executeTool(session, approvalTool, request("approval-unresolved"), { approval: async () => approval });
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(executed).toBe(false);
    expect(session.snapshot().entries.some((entry) => entry.kind === "message" && entry.role === "toolResult")).toBe(false);
    resolveApproval(false);
    await operation;
    session.close();
  });
});

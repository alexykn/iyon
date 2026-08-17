import { describe, expect, test } from "bun:test";

import { installIyonVirtualModules } from "../src/virtual-modules.ts";

installIyonVirtualModules();

const core = await import("iyon:core");

describe("T4 core façade", () => {
  test("streams a model turn and exposes the canonical snapshot", async () => {
    const session = new core.KernelSession({ id: 21 });
    expect(
      session.appendMessage({
        role: "user",
        content: [{ type: "text", text: "hello" }],
      }),
    ).toBe(1);

    const turn = session.beginModelTurn({
      request: {
        messages: [{ role: "user", content: [{ type: "text", text: "hello" }] }],
        tools: [],
        params: {},
        metadata: {},
      },
    });
    await turn.push({ type: "textDelta", contentIndex: 0, delta: "world" });
    await turn.push({ type: "done", stopReason: "stop" });
    await turn.finish();

    const events = [];
    for await (const event of session.events()) {
      events.push(event.type);
      if (event.type === "turnFinished") {
        break;
      }
    }
    expect(events).toEqual([
      "messageStarted",
      "messageDelta",
      "messageFinished",
      "turnFinished",
    ]);
    expect(session.snapshot().entries.map((entry) => entry.role)).toEqual([
      "user",
      "assistant",
    ]);
    session.close();
  });

  test("deliverUserMessage appends and emits the canonical user triple", async () => {
    const session = new core.KernelSession({ id: 23 });
    expect(session.deliverUserMessage("wh")).toBe(1);
    expect(session.snapshot().entries.map((entry) => entry.role)).toEqual(["user"]);
    session.close();
    const events = [];
    for await (const event of session.events()) events.push(event);
    expect(events.map((event) => event.type)).toEqual([
      "messageStarted",
      "messageDelta",
      "messageFinished",
    ]);
    expect(events[0]).toMatchObject({ type: "messageStarted", role: "user", messageId: 1 });
    expect(events[1]).toMatchObject({ type: "messageDelta", delta: { type: "text", text: "wh" } });
  });

  test("drives approval and records one tool result", async () => {
    const session = new core.KernelSession({ id: 22 });
    const tool = session.prepareToolExecution({
      turnId: 1 as never,
      messageId: 2 as never,
      toolCallId: "call-1" as never,
      toolName: "fake",
      arguments: { path: "README" },
    });
    tool.prepared({ path: "README" });
    tool.start();
    const approval = tool.requestApproval({ type: "required", reason: "test" });
    expect(approval?.status.type).toBe("pending");
    tool.approve(approval!.id);
    tool.finish({
      content: [{ type: "text", text: "contents" }],
      details: { bytes: 8 },
      isError: false,
    });

    const entries = session.snapshot().entries;
    expect(entries.filter((entry) => entry.role === "toolResult")).toHaveLength(1);
    expect(tool.state()).toBe("finished");
    session.close();
  });

  test("pre-aborted operations do not start native work", async () => {
    const controller = new AbortController();
    controller.abort();
    let started = false;
    await expect(
      core.runWithAbortSignal(controller.signal, {
        run: async () => {
          started = true;
          return "unexpected";
        },
        cancel: () => {},
      }),
    ).rejects.toMatchObject({ code: "ION_CANCELLED" });
    expect(started).toBe(false);
  });
});

import { describe, expect, test } from "bun:test";
import type { FrontendEvent } from "../src/contracts.ts";
import {
  advance,
  closeFixture,
  draft,
  openFixture,
  send,
  toolStatusCount,
  transcriptLines,
  type PublicAppFixture,
} from "./public_app_fixtures.ts";

async function withFixture<T>(width: number, height: number, callback: (fixture: PublicAppFixture) => Promise<T>): Promise<T> {
  const fixture = await openFixture(width, height);
  try {
    return await callback(fixture);
  } finally {
    await closeFixture(fixture);
  }
}

async function sendAll(fixture: PublicAppFixture, events: readonly FrontendEvent[]): Promise<void> {
  for (const event of events) await send(fixture, event);
}

function position(lines: readonly string[], text: string): number {
  const index = lines.findIndex((line) => line.includes(text));
  if (index < 0) throw new Error(`missing ${text} in ${lines.join("\n")}`);
  return index;
}

describe("Iyon public native TUI", () => {
  test("is drivable through the public TUI harness", async () => {
    await withFixture(40, 12, async ({ app, harness }) => {
      for (const key of "hello") harness.pressKey(key);
      harness.pressKey("Enter");
      const action = await harness.nextAction();
      expect(action).toEqual({ actionId: "submit", payload: "hello" });
      await app.handleAction({ type: "submit", text: action?.payload ?? "" });
      expect(await app.composer.text()).toBe("");
      expect(harness.screenRows().at(-1)).toContain("effort: Medium");
      expect(transcriptLines(harness).filter((line) => line.includes("hello"))).toHaveLength(1);
    });
  });

  test("flushes pending assistant smoothing before a tool card", async () => {
    await withFixture(60, 20, async (fixture) => {
      await sendAll(fixture, [
        { type: "turnStarted" },
        { type: "assistantDelta", text: "assistant tail" },
        { type: "toolCallStarted", toolCallId: "boundary-tool", toolName: "bash", arguments: { command: "true" } },
      ]);
      advance(fixture, 16, 8);
      const lines = transcriptLines(fixture.harness);
      expect(position(lines, "assistant tail")).toBeLessThan(position(lines, "$ true"));
    });
  });

  test("preserves a partial assistant stream across cancellation", async () => {
    await withFixture(40, 12, async (fixture) => {
      await sendAll(fixture, [{ type: "turnStarted" }, { type: "assistantDelta", text: "cancelled assistant tail" }, { type: "turnCancelled" }]);
      advance(fixture, 16, 8);
      expect(position(transcriptLines(fixture.harness), "cancelled assistant tail")).toBeGreaterThanOrEqual(0);
      expect(fixture.app.state.working).toBe(false);
    });
  });

  test("keeps the composer below completed tool history", async () => {
    await withFixture(60, 12, async (fixture) => {
      const key = draft(1, 0);
      await sendAll(fixture, [
        { type: "turnStarted" },
        { type: "toolCallPreparing", key, toolName: "bash" },
        { type: "toolCallPrepared", key, toolCallId: "completed-tool", toolName: "bash", arguments: { command: "printf output" } },
        { type: "toolCallStarted", toolCallId: "completed-tool", toolName: "bash", arguments: { command: "printf output" } },
        { type: "toolResult", toolCallId: "completed-tool", toolName: "bash", text: "final output", details: {}, isError: false },
      ]);
      const rows = fixture.harness.screenRows();
      expect(position(rows, "final output")).toBeLessThan(rows.length - 1);
      expect(rows.some((line) => line.includes("effort"))).toBe(true);
    });
  });

  test("shows a streamed draft before execution and reuses one card", async () => {
    await withFixture(80, 20, async (fixture) => {
      const key = draft(7, 0);
      await send(fixture, { type: "turnStarted" });
      await send(fixture, { type: "toolCallPreparing", key, toolName: "bash" });
      expect(toolStatusCount(transcriptLines(fixture.harness), "bash", "preparing")).toBe(1);
      await send(fixture, { type: "toolCallPrepared", key, toolCallId: "call-b", toolName: "bash", arguments: { command: "echo b" } });
      expect(toolStatusCount(transcriptLines(fixture.harness), "echo b", "ready")).toBe(1);
      await send(fixture, { type: "toolCallStarted", toolCallId: "call-b", toolName: "bash", arguments: { command: "echo b" } });
      expect(toolStatusCount(transcriptLines(fixture.harness), "echo b", "running")).toBe(1);
      expect(transcriptLines(fixture.harness).filter((line) => line.includes("echo b") && line.includes("—")).length).toBe(1);
    });
  });

  test("keeps prepared tool order while only the started tool runs", async () => {
    await withFixture(80, 20, async (fixture) => {
      const bash = draft(7, 0);
      const read = draft(7, 1);
      await sendAll(fixture, [
        { type: "turnStarted" },
        { type: "toolCallPreparing", key: bash, toolName: "bash" },
        { type: "toolCallPrepared", key: bash, toolCallId: "call-b", toolName: "bash", arguments: { command: "echo b" } },
        { type: "toolCallPreparing", key: read, toolName: "read" },
        { type: "toolCallPrepared", key: read, toolCallId: "call-r", toolName: "read", arguments: { path: "a.txt" } },
        { type: "toolCallStarted", toolCallId: "call-r", toolName: "read", arguments: { path: "a.txt" } },
      ]);
      const lines = transcriptLines(fixture.harness);
      expect(position(lines, "read a.txt — running")).toBeGreaterThan(position(lines, "echo b — ready"));
      expect(toolStatusCount(lines, "echo b", "running")).toBe(0);
    });
  });

  test("updates a prepared approval card in place", async () => {
    await withFixture(80, 20, async (fixture) => {
      const key = draft(7, 0);
      await sendAll(fixture, [
        { type: "turnStarted" },
        { type: "toolCallPreparing", key, toolName: "read" },
        { type: "toolCallPrepared", key, toolCallId: "approval-call", toolName: "read", arguments: { path: "secrets.txt" } },
        { type: "toolApprovalRequested", approvalId: 42, toolCallId: "approval-call", toolName: "read", arguments: { path: "secrets.txt" } },
      ]);
      const lines = transcriptLines(fixture.harness);
      expect(toolStatusCount(lines, "read", "waiting for approval")).toBe(1);
      expect(lines.filter((line) => line.includes("read") && line.includes(" — ")).length).toBe(1);
    });
  });

  test("freezes a preparing tool as cancelled", async () => {
    await withFixture(80, 20, async (fixture) => {
      const key = draft(7, 0);
      await sendAll(fixture, [{ type: "turnStarted" }, { type: "toolCallPreparing", key, toolName: "read" }, { type: "turnCancelled" }]);
      const lines = transcriptLines(fixture.harness);
      expect(toolStatusCount(lines, "read", "cancelled")).toBe(1);
      expect(toolStatusCount(lines, "read", "finished")).toBe(0);
    });
  });

  test("does not mark a cancelled running tool finished", async () => {
    await withFixture(80, 20, async (fixture) => {
      const key = draft(7, 0);
      await sendAll(fixture, [
        { type: "turnStarted" },
        { type: "toolCallPreparing", key, toolName: "read" },
        { type: "toolCallPrepared", key, toolCallId: "running-call", toolName: "read", arguments: { path: "a.txt" } },
        { type: "toolCallStarted", toolCallId: "running-call", toolName: "read", arguments: { path: "a.txt" } },
        { type: "turnCancelled" },
      ]);
      const lines = transcriptLines(fixture.harness);
      expect(toolStatusCount(lines, "read", "cancelled")).toBe(1);
      expect(toolStatusCount(lines, "read", "finished")).toBe(0);
    });
  });

  test("renders an error result in the prepared card without an orphan row", async () => {
    await withFixture(80, 20, async (fixture) => {
      const key = draft(7, 0);
      await sendAll(fixture, [
        { type: "turnStarted" },
        { type: "toolCallPreparing", key, toolName: "read" },
        { type: "toolCallPrepared", key, toolCallId: "error-call", toolName: "read", arguments: { path: "missing.txt" } },
        { type: "toolResult", toolCallId: "error-call", toolName: "read", text: "missing", details: {}, isError: true },
      ]);
      const lines = transcriptLines(fixture.harness);
      expect(lines.filter((line) => line.includes("read failed")).length).toBe(1);
      expect(lines.filter((line) => line.includes("read") && line.includes(" — ")).length).toBe(0);
    });
  });

  test("forces a missing tool result final at turn end", async () => {
    await withFixture(80, 20, async (fixture) => {
      const key = draft(8, 0);
      await sendAll(fixture, [
        { type: "turnStarted" },
        { type: "toolCallPreparing", key, toolName: "read" },
        { type: "toolCallPrepared", key, toolCallId: "missing-result", toolName: "read", arguments: { path: "a.txt" } },
        { type: "toolCallStarted", toolCallId: "missing-result", toolName: "read", arguments: { path: "a.txt" } },
        { type: "turnFinished" },
      ]);
      expect(toolStatusCount(transcriptLines(fixture.harness), "read", "failed")).toBe(1);
      expect(fixture.app.state.liveTools.get("8:0")?.frozen).toBe(true);
    });
  });

  test("keeps long Markdown code from pinning the composer", async () => {
    await withFixture(40, 20, async (fixture) => {
      await send(fixture, { type: "assistantDelta", text: "```rust\nthis_is_a_ridiculously_long_function_call();\n```\n" });
      advance(fixture, 16, 80);
      const rows = fixture.harness.screenRows();
      expect(rows.some((line) => line.includes("this_is_a_ridiculously_long_function"))).toBe(true);
      expect(rows.some((line) => line.includes("effort"))).toBe(true);
    });
  });

  test("flushes buffered assistant text before Goodbye", async () => {
    await withFixture(40, 12, async (fixture) => {
      await send(fixture, { type: "assistantDelta", text: "buffered assistant" });
      await fixture.app.handleAction({ type: "requestExit" });
      const rows = fixture.harness.screenRows();
      expect(position(rows, "buffered assistant")).toBeLessThan(position(rows, "Goodbye."));
      expect(fixture.harness.exited()).toBe(true);
    });
  });

  test("keeps an approval prompt beside a user batch delivered after a tool", async () => {
    await withFixture(60, 20, async (fixture) => {
      const key = draft(9, 0);
      await sendAll(fixture, [
        { type: "turnStarted" },
        { type: "toolCallPreparing", key, toolName: "read" },
        { type: "toolCallPrepared", key, toolCallId: "approval-tail", toolName: "read", arguments: { path: "a.txt" } },
        { type: "toolApprovalRequested", approvalId: 9, toolCallId: "approval-tail", toolName: "read", arguments: { path: "a.txt" } },
        { type: "userMessage", text: "last user bubble" },
      ]);
      const lines = transcriptLines(fixture.harness);
      expect(position(lines, "last user bubble")).toBeGreaterThanOrEqual(0);
      expect(position(lines, "Approve read?")).toBeGreaterThanOrEqual(0);
    });
  });

  test("keeps a steered user message before the assistant stream tail", async () => {
    await withFixture(60, 20, async (fixture) => {
      await sendAll(fixture, [
        { type: "turnStarted" },
        { type: "userMessage", text: "initial user" },
        { type: "toolCallStarted", toolCallId: "steer-tool", toolName: "bash", arguments: { command: "true" } },
        { type: "toolResult", toolCallId: "steer-tool", toolName: "bash", text: "tool output", details: {}, isError: false },
        { type: "userMessage", text: "steered user" },
        { type: "assistantDelta", text: "assistant after steering" },
      ]);
      advance(fixture, 16, 40);
      const lines = transcriptLines(fixture.harness);
      expect(position(lines, "steered user")).toBeLessThan(position(lines, "assistant after steering"));
    });
  });

  test("keeps streaming assistant content contiguous while the composer collapses", async () => {
    await withFixture(40, 20, async (fixture) => {
      await send(fixture, { type: "assistantDelta", text: "assistant before composer" });
      fixture.harness.paste(Array.from({ length: 12 }, (_, index) => `line ${index}`).join("\n"));
      const action = await fixture.harness.nextAction();
      await fixture.app.handleAction({ type: "composerPaste", text: action?.payload ?? "" });
      advance(fixture, 16, 40);
      const rows = fixture.harness.screenRows();
      expect(rows.some((line) => line.includes("assistant before composer"))).toBe(true);
      expect(rows.length).toBe(20);
    });
  });

  test("consumes history slack before transferring a shrinking composer", async () => {
    await withFixture(40, 20, async (fixture) => {
      await send(fixture, { type: "userMessage", text: "history slack" });
      await send(fixture, { type: "assistantDelta", text: "history assistant" });
      fixture.harness.paste("one\ntwo\nthree\nfour\nfive");
      const action = await fixture.harness.nextAction();
      await fixture.app.handleAction({ type: "composerPaste", text: action?.payload ?? "" });
      const rows = fixture.harness.screenRows();
      expect(rows.some((line) => line.includes("history slack"))).toBe(true);
      expect(rows.length).toBe(20);
    });
  });

  test("shows multiline tool updates through the same card", async () => {
    await withFixture(60, 20, async (fixture) => {
      await sendAll(fixture, [
        { type: "turnStarted" },
        { type: "toolCallStarted", toolCallId: "update-tool", toolName: "bash", arguments: { command: "true" } },
        { type: "toolCallUpdated", toolCallId: "update-tool", update: { type: "text", text: "running\nsecond" } },
      ]);
      const lines = transcriptLines(fixture.harness);
      expect(position(lines, "running")).toBeGreaterThanOrEqual(0);
      expect(position(lines, "second")).toBeGreaterThanOrEqual(0);
      expect(lines.filter((line) => line.includes("$ true") && line.includes(" — ")).length).toBe(1);
    });
  });

  test("renders short Markdown paragraphs, lists, and tables without replacing native history", async () => {
    await withFixture(40, 20, async (fixture) => {
      await sendAll(fixture, [
        { type: "assistantDelta", text: "intro paragraph\n\n- one\n- two\n\n| A | B |\n| --- | --- |\n| 1 | 2 |" },
      ]);
      advance(fixture, 16, 100);
      const rows = fixture.harness.screenRows();
      expect(rows.some((line) => line.includes("intro paragraph"))).toBe(true);
      expect(rows.some((line) => line.includes("A"))).toBe(true);
      expect(rows.some((line) => line.includes("effort"))).toBe(true);
    });
  });

  test("shows pending steering beside the native working activity", async () => {
    await withFixture(60, 20, async (fixture) => {
      await fixture.app.handleAction({ type: "submit", text: "initial" });
      await fixture.app.handleAction({ type: "submit", text: "steer" });
      const rows = fixture.harness.screenRows();
      expect(rows.some((line) => line.includes("Queue: steer"))).toBe(true);
      expect(rows.some((line) => line.includes("waiting"))).toBe(true);
    });
  });
});

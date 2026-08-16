import { describe, expect, test } from "bun:test";
import { installIyonVirtualModules } from "@iyon/runtime";
import type { SessionEntry } from "@iyon/sdk";
import { SessionDriver } from "./session-driver.ts";
import { emptyRequest, textTurn } from "./fixtures.ts";

installIyonVirtualModules();

describe("bundled agent public harness", () => {
  test("captures a scripted request and complete assistant result", async () => {
    const driver = await SessionDriver.create(101);
    driver.enqueue(textTurn("hello"));

    const result = await driver.runTurn({ ...emptyRequest(), messages: [{ role: "user", content: [{ type: "text", text: "hi" }] }] });

    expect(driver.provider.requests).toHaveLength(1);
    expect(driver.provider.requests[0]?.messages[0]).toEqual({ role: "user", content: [{ type: "text", text: "hi" }] });
    expect(result.stopReason).toBe("stop");
    expect(driver.snapshot().entries.some((entry: SessionEntry) => entry.kind === "message" && entry.role === "assistant")).toBe(true);
    driver.close();
  });

  test("cancellation terminates a pending public turn", async () => {
    const driver = await SessionDriver.create(102);
    driver.enqueue({ waitForAbort: true });
    const controller = new AbortController();
    const pending = driver.runTurn(emptyRequest(), controller.signal);
    await Bun.sleep(1);
    controller.abort();

    const result = await pending;
    expect(result.cancelled).toBe(true);
    expect(result.assistantMessage.role).toBe("assistant");
    driver.close();
  });
});

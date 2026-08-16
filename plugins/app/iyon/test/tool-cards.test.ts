import { describe, expect, test } from "bun:test";
import { ToolCardStore } from "../src/tool-cards.ts";

describe("generic tool cards", () => {
  test("reuses one draft when the provider ID arrives late", () => {
    const cards = new ToolCardStore(); const key = { messageId: 4, contentIndex: 0 };
    cards.preparing(key); cards.arguments(key, "{", undefined, "generic"); cards.prepared(key, "call-1", "generic", { value: 1 }); cards.started("call-1", "generic", { value: 1 });
    expect(cards.values()).toHaveLength(1); expect(cards.get("call-1")?.status).toBe("running");
  });
  test("freezes results without tool-name presentation branches", () => {
    const cards = new ToolCardStore(); cards.started("call-1", "any-tool", {}); cards.update("call-1", { type: "text", text: "output" });
    const result = cards.result("call-1", "any-tool", "done", { detail: true }, false);
    expect(result).toMatchObject({ status: "finished", text: "done", frozen: true, isError: false });
  });
});

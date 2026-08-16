import { describe, expect, test } from "bun:test";
import { ApprovalStore, pendingApproval } from "../src/approvals.ts";

describe("approvals", () => {
  test("keeps approval and tool identifiers independent", () => {
    const approvals = new ApprovalStore(); const value = pendingApproval(7, "tool-7", "generic", { path: "x" });
    approvals.request(value); expect(approvals.get(7)).toEqual(value); expect(approvals.resolve(7)?.toolCallId).toBe("tool-7"); expect(approvals.values()).toHaveLength(0);
  });
});

import { describe, expect, test } from "bun:test";
import { bashApprovalPolicy, bashCommandUsesSudo } from "../../src/tools/policy.ts";

describe("tool approval policy", () => {
  test("keeps sudo detection in the bash policy contribution", () => {
    expect(bashCommandUsesSudo({ command: "echo hi" })).toBe(false);
    expect(bashCommandUsesSudo({ command: "sudo rm file" })).toBe(true);
    expect(bashCommandUsesSudo({ command: "echo sudoers" })).toBe(false);
    expect(bashApprovalPolicy.approval("bash", { command: "sudo whoami" }, { type: "notRequired" })).toMatchObject({ type: "required" });
    expect(bashApprovalPolicy.approval("bash", { command: "echo ok" }, { type: "notRequired" })).toEqual({ type: "notRequired" });
  });
});

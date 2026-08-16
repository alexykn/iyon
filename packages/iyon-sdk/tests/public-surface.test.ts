import { describe, expect, test } from "bun:test";
import type { ModelApi, ModelStreamEvent } from "../src/index.ts";
import type { CoreEvent, KernelSession } from "../src/index.ts";

describe("T4 public SDK surface", () => {
  test("exports protocol and core contracts", () => {
    const event: ModelStreamEvent = { type: "done", stopReason: "stop" };
    const provider: ModelApi = {
      stream: async function* () {
        yield event;
      },
    };
    const coreEvent: CoreEvent = { type: "agentStarted" };
    const session: KernelSession | undefined = undefined;
    expect(provider.stream).toBeFunction();
    expect(coreEvent.type).toBe("agentStarted");
    expect(session).toBeUndefined();
  });
});

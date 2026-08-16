import { describe, expect, test } from "bun:test";
import type {
  ContentBlock,
  ModelStreamEvent,
  ModelMessage,
} from "../src/api.ts";
import type { CoreEvent, MessageId, TurnId } from "../src/core.ts";

describe("T4 protocol contracts", () => {
  test("protocol unions preserve discriminants and camelCase fields", () => {
    const message: ModelMessage = {
      role: "toolResult",
      toolCallId: "call-1",
      toolName: "read",
      content: [{ type: "text", text: "ok" }],
      isError: false,
    };
    const event: ModelStreamEvent = {
      type: "toolCallDelta",
      contentIndex: 0,
      id: "call-1",
      name: "read",
      argumentsDelta: "{}",
    };
    const image: ContentBlock = {
      type: "image",
      data: new Uint8Array([1, 2]),
      mimeType: "image/png",
    };

    expect(message.role).toBe("toolResult");
    expect(event.type).toBe("toolCallDelta");
    expect(image.type).toBe("image");
  });

  test("core events narrow independently from protocol events", () => {
    const event: CoreEvent = {
      type: "messageDelta",
      turnId: 1 as TurnId,
      messageId: 2 as MessageId,
      delta: { type: "text", text: "hello" },
    };

    if (event.type !== "messageDelta" || event.delta.type !== "text") {
      throw new Error("unexpected event shape");
    }
    expect(event.delta.text).toBe("hello");
  });
});

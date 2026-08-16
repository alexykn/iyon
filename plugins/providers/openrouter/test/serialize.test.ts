import { describe, expect, test } from "bun:test";
import { buildRequestBody } from "../src/serialize.ts";

describe("OpenRouter serialization", () => {
  test("serializes full model requests", () => {
    expect(buildRequestBody({
      systemPrompt: "be concise",
      messages: [
        { role: "user", content: [{ type: "text", text: "hello" }, { type: "image", data: new Uint8Array([1, 2]), mimeType: "image/png" }] },
        { role: "assistant", content: [{ type: "toolCall", id: "call-1", name: "lookup", arguments: { q: "x" } }] },
        { role: "toolResult", toolCallId: "call-1", toolName: "lookup", content: [{ type: "text", text: "result" }], isError: false },
      ],
      tools: [{ name: "lookup", description: "find", inputSchema: { type: "object" } }],
      params: { temperature: 0.2, maxTokens: 12, reasoning: "high" },
    }, "model")).toEqual({
      model: "model",
      messages: [
        { role: "system", content: "be concise" },
        { role: "user", content: [{ type: "text", text: "hello" }, { type: "image_url", image_url: { url: "data:image/png;base64,AQI=" } }] },
        { role: "assistant", content: null, tool_calls: [{ id: "call-1", type: "function", function: { name: "lookup", arguments: "{\"q\":\"x\"}" } }] },
        { role: "tool", tool_call_id: "call-1", content: "result" },
      ],
      stream: true,
      tool_choice: "auto",
      tools: [{ type: "function", function: { name: "lookup", description: "find", parameters: { type: "object" } } }],
      temperature: 0.2,
      max_tokens: 12,
      reasoning: { effort: "high", exclude: false },
    });
  });
});

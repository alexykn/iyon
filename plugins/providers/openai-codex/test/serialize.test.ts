import { describe, expect, test } from "bun:test";
import { buildRequestBody } from "../src/serialize.ts";

describe("Codex serialization", () => {
  test("serializes Responses input and tool settings", () => {
    expect(buildRequestBody({
      systemPrompt: "system",
      messages: [{ role: "user", content: [{ type: "text", text: "hello" }, { type: "image", data: new Uint8Array([255]), mimeType: "image/jpeg" }] }],
      tools: [{ name: "lookup", description: "find", inputSchema: { type: "object" } }],
      params: { reasoning: "low", maxTokens: 99 },
    }, "session")).toMatchObject({ model: "gpt-5.3-codex", prompt_cache_key: "session", max_output_tokens: 99, reasoning: { effort: "low" }, tools: [{ name: "lookup" }] });
  });
});

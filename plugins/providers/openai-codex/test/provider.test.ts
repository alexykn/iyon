import { describe, expect, test } from "bun:test";
import { OpenAICodexProvider } from "../src/provider.ts";
import { collectEvents, expectEventSequence } from "../../../../packages/iyon-runtime/test/providers/fixtures.ts";

function fragmented(value: string): Response {
  const bytes = new TextEncoder().encode(value);
  return new Response(new ReadableStream<Uint8Array>({
    start(controller) { for (const byte of bytes) controller.enqueue(new Uint8Array([byte])); controller.close(); },
  }));
}

describe("Codex Responses streaming", () => {
  test("reconciles fragmented text and function arguments", async () => {
    const sse = [
      `data: ${JSON.stringify({ type: "response.output_item.added", item: { type: "message" } })}\n\n`,
      `data: ${JSON.stringify({ type: "response.output_text.delta", delta: "hello" })}\n\n`,
      `data: ${JSON.stringify({ type: "response.output_item.done", item: { type: "message" } })}\n\n`,
      `data: ${JSON.stringify({ type: "response.output_item.added", item: { type: "function_call", call_id: "call-1", name: "lookup", arguments: "{" } })}\n\n`,
      `data: ${JSON.stringify({ type: "response.function_call_arguments.delta", delta: "\"q\":\"x\"" })}\n\n`,
      `data: ${JSON.stringify({ type: "response.function_call_arguments.done", arguments: "{\"q\":\"x\"}" })}\n\n`,
      `data: ${JSON.stringify({ type: "response.output_item.done", item: { type: "function_call" } })}\n\n`,
      `data: ${JSON.stringify({ type: "response.completed", response: { status: "completed", usage: { input_tokens: 8, output_tokens: 2, input_tokens_details: { cached_tokens: 3 } } } })}\n\n`,
    ].join("");
    const provider = new OpenAICodexProvider({ accessToken: "redacted", accountId: "account", sessionId: () => "session", fetch: async () => fragmented(sse) });
    const events = await collectEvents(provider.stream({ messages: [], tools: [] }));
    expectEventSequence(events, [
      { type: "started" },
      { type: "textStart", contentIndex: 0 },
      { type: "textDelta", contentIndex: 0, delta: "hello" },
      { type: "textEnd", contentIndex: 0, text: "hello" },
      { type: "toolCallStart", contentIndex: 0, id: "call-1", name: "lookup" },
      { type: "toolCallDelta", contentIndex: 0, id: "call-1", name: "lookup", argumentsDelta: "\"q\":\"x\"" },
      { type: "toolCallDelta", contentIndex: 0, id: "call-1", name: "lookup", argumentsDelta: "}" },
      { type: "toolCallEnd", contentIndex: 0, id: "call-1", name: "lookup", arguments: { q: "x" } },
      { type: "usage", usage: { inputTokens: 5, outputTokens: 2, cacheReadTokens: 3, cacheWriteTokens: 0 } },
      { type: "done", stopReason: "toolUse" },
    ]);
  });
});

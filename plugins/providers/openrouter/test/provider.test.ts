import { describe, expect, test } from "bun:test";
import { OpenRouterProvider } from "../src/provider.ts";
import { collectEvents, expectEventSequence } from "../../../../packages/iyon-runtime/test/providers/fixtures.ts";

function responseFromChunks(value: string, size = 1): Response {
  const bytes = new TextEncoder().encode(value);
  const body = new ReadableStream<Uint8Array>({
    start(controller) {
      for (let offset = 0; offset < bytes.length; offset += size) controller.enqueue(bytes.slice(offset, offset + size));
      controller.close();
    },
  });
  return new Response(body, { status: 200 });
}

describe("OpenRouter streaming", () => {
  test("parses fragmented text, reasoning, usage, and done frames", async () => {
    const frames = [
      ": comment\ndata: {\"choices\":[{\"delta\":{\"reasoning\":\"think\"}}]}\n\n",
      "data: {\"choices\":[{\"delta\":{\"content\":\"hé\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":3,\"prompt_tokens_details\":{\"cached_tokens\":4}}}\n\n",
      "data: [DONE]\n\n",
    ].join("");
    const provider = new OpenRouterProvider({ apiKey: "redacted", fetch: async (_input, init) => { expect(init?.headers).toMatchObject({ accept: "text/event-stream" }); return responseFromChunks(frames); }, baseUrl: "https://example.test" });
    const events = await collectEvents(provider.stream({ messages: [{ role: "user", content: [{ type: "text", text: "hi" }] }], tools: [] }));
    expectEventSequence(events, [
      { type: "started" },
      { type: "thinkingDelta", contentIndex: 0, delta: "think" },
      { type: "textDelta", contentIndex: 0, delta: "hé" },
      { type: "usage", usage: { inputTokens: 6, outputTokens: 3, cacheReadTokens: 4, cacheWriteTokens: 0 } },
      { type: "done", stopReason: "stop" },
    ]);
  });

  test("retries transient statuses but not authentication", async () => {
    let attempts = 0;
    const provider = new OpenRouterProvider({ apiKey: "redacted", sleep: async () => undefined, fetch: async () => { attempts += 1; return attempts === 1 ? new Response("busy", { status: 503 }) : responseFromChunks("data: [DONE]\n\n"); } });
    await collectEvents(provider.stream({ messages: [], tools: [] }));
    expect(attempts).toBe(2);
    const auth = new OpenRouterProvider({ apiKey: "redacted", fetch: async () => new Response("no", { status: 401 }) });
    await expect(collectEvents(auth.stream({ messages: [], tools: [] }))).rejects.toMatchObject({ kind: "authentication" });
  });
});

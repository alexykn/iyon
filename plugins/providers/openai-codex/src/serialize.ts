import type { ContentBlock, ModelMessage, ModelRequest, ModelToolSpec, ReasoningLevel } from "iyon:api";
import type { JsonValue } from "@iyon/sdk";

export const DEFAULT_MODEL = "gpt-5.3-codex";

export function buildRequestBody(request: ModelRequest, sessionId: string, model = DEFAULT_MODEL): Record<string, JsonValue> {
  const params = request.params ?? {};
  const body: Record<string, JsonValue> = {
    model,
    store: false,
    stream: true,
    instructions: request.systemPrompt ?? "",
    input: request.messages.flatMap(convertMessage),
    tool_choice: "auto",
    parallel_tool_calls: true,
    text: { verbosity: "low" },
    include: ["reasoning.encrypted_content"],
    prompt_cache_key: sessionId,
    reasoning: { effort: params.reasoning ?? ("medium" satisfies ReasoningLevel), summary: "auto" },
  };
  if (params.temperature !== undefined) body.temperature = params.temperature;
  if (params.maxTokens !== undefined) body.max_output_tokens = params.maxTokens;
  if (request.tools.length > 0) body.tools = request.tools.map(convertTool);
  return body;
}

export function convertMessage(message: ModelMessage): JsonValue[] {
  if (message.role === "user") return [{ role: "user", content: convertContentBlocks(message.content, true) }];
  if (message.role === "toolResult") {
    return [{ type: "function_call_output", call_id: message.toolCallId, output: message.content.filter(isText).map((block) => block.text).join("\n") }];
  }
  return message.content.flatMap((block): JsonValue[] => {
    if (block.type === "text") return [{ type: "message", role: "assistant", content: [{ type: "output_text", text: block.text, annotations: [] }], status: "completed" }];
    if (block.type === "toolCall") return [{ type: "function_call", call_id: block.id, name: block.name, arguments: JSON.stringify(block.arguments) }];
    return [];
  });
}

export function convertTool(tool: ModelToolSpec): JsonValue {
  return { type: "function", name: tool.name, description: tool.description, parameters: tool.inputSchema, strict: false };
}

export function convertContentBlocks(content: readonly ContentBlock[], user: boolean): JsonValue[] {
  return content.flatMap((block): JsonValue[] => {
    if (block.type === "text") return [{ type: user ? "input_text" : "output_text", text: block.text }];
    if (block.type === "image" && user) return [{ type: "input_image", detail: "auto", image_url: `data:${block.mimeType};base64,${bytesToBase64(block.data)}` }];
    return [];
  });
}

function isText(block: ContentBlock): block is Extract<ContentBlock, { type: "text" }> { return block.type === "text"; }
function bytesToBase64(bytes: Uint8Array): string { let binary = ""; for (const byte of bytes) binary += String.fromCharCode(byte); return btoa(binary); }

import type { ContentBlock, ModelMessage, ModelRequest, ModelToolSpec } from "iyon:api";
import type { JsonValue } from "@iyon/sdk";

export function buildRequestBody(request: ModelRequest, modelId: string): Record<string, JsonValue> {
  const messages: JsonValue[] = [];
  if (request.systemPrompt?.trim()) messages.push({ role: "system", content: request.systemPrompt });
  for (const message of request.messages) messages.push(convertMessage(message));

  const params = request.params ?? {};
  const body: Record<string, JsonValue> = {
    model: modelId,
    messages,
    stream: true,
    tool_choice: "auto",
  };
  if (request.tools.length > 0) body.tools = request.tools.map(convertTool);
  if (params.temperature !== undefined) body.temperature = params.temperature;
  if (params.maxTokens !== undefined) body.max_tokens = params.maxTokens;
  if (params.reasoning !== undefined) body.reasoning = { effort: params.reasoning, exclude: false };
  return body;
}

export function convertMessage(message: ModelMessage): JsonValue {
  if (message.role === "user") return { role: "user", content: convertUserContent(message.content) };
  if (message.role === "toolResult") {
    return {
      role: "tool",
      tool_call_id: message.toolCallId,
      content: message.content.filter(isText).map((block) => block.text).join("\n"),
    };
  }
  return convertAssistant(message.content);
}

export function convertUserContent(content: readonly ContentBlock[]): JsonValue[] {
  return content.flatMap((block): JsonValue[] => {
    if (block.type === "text") return [{ type: "text", text: block.text }];
    if (block.type === "image") return [{ type: "image_url", image_url: { url: `data:${block.mimeType};base64,${bytesToBase64(block.data)}` } }];
    return [];
  });
}

export function convertAssistant(content: readonly ContentBlock[]): JsonValue {
  const text = content.filter(isText).map((block) => block.text).filter(Boolean);
  const toolCalls = content.filter((block): block is Extract<ContentBlock, { type: "toolCall" }> => block.type === "toolCall").map((block) => ({
    id: block.id,
    type: "function",
    function: { name: block.name, arguments: JSON.stringify(block.arguments) },
  }));
  return {
    role: "assistant",
    content: toolCalls.length === 0 ? text.join("\n") : text.length === 0 ? null : text.join("\n"),
    ...(toolCalls.length > 0 ? { tool_calls: toolCalls } : {}),
  };
}

export function convertTool(tool: ModelToolSpec): JsonValue {
  return { type: "function", function: { name: tool.name, description: tool.description, parameters: tool.inputSchema } };
}

function isText(block: ContentBlock): block is Extract<ContentBlock, { type: "text" }> { return block.type === "text"; }

function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

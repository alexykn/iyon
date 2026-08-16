/** The protocol-only model boundary implemented by providers. */

export type ReasoningLevel =
  | "none"
  | "minimal"
  | "low"
  | "medium"
  | "high"
  | "xhigh"
  | "max";

export type CacheRetention = "none" | "short" | "long";

export type StopReason = "stop" | "length" | "toolUse" | "error" | "aborted";

export type ContentBlock =
  | { type: "text"; text: string }
  | { type: "image"; data: Uint8Array; mimeType: string }
  | { type: "thinking"; text: string }
  | { type: "toolCall"; id: string; name: string; arguments: JsonValue };

export type ModelMessage =
  | { role: "user"; content: ContentBlock[] }
  | { role: "assistant"; content: ContentBlock[] }
  | {
      role: "toolResult";
      toolCallId: string;
      toolName: string;
      content: ContentBlock[];
      isError: boolean;
    };

export interface ModelToolSpec {
  name: string;
  description: string;
  inputSchema: JsonValue;
}

export interface ModelParams {
  temperature?: number;
  maxTokens?: number;
  reasoning?: ReasoningLevel;
  cacheRetention?: CacheRetention;
}

export interface ModelMetadata {
  sessionId?: string;
  userId?: string;
}

export interface ModelRequest {
  systemPrompt?: string;
  messages: ModelMessage[];
  tools: ModelToolSpec[];
  params?: ModelParams;
  metadata?: ModelMetadata;
}

export interface Usage {
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
}

export type ModelStreamEvent =
  | { type: "started" }
  | { type: "textStart"; contentIndex: number }
  | { type: "textDelta"; contentIndex: number; delta: string }
  | { type: "textEnd"; contentIndex: number; text: string }
  | { type: "thinkingStart"; contentIndex: number }
  | { type: "thinkingDelta"; contentIndex: number; delta: string }
  | { type: "thinkingEnd"; contentIndex: number; text: string }
  | {
      type: "toolCallStart";
      contentIndex: number;
      id?: string;
      name?: string;
    }
  | {
      type: "toolCallDelta";
      contentIndex: number;
      id?: string;
      name?: string;
      argumentsDelta: string;
    }
  | {
      type: "toolCallEnd";
      contentIndex: number;
      id: string;
      name: string;
      arguments: JsonValue;
    }
  | { type: "usage"; usage: Usage }
  | { type: "done"; stopReason: StopReason }
  | { type: "error"; message: string };

export type ModelErrorKind =
  | "invalidRequest"
  | "authentication"
  | "rateLimited"
  | "provider"
  | "transport"
  | "cancelled"
  | "unknown";

export interface ModelError {
  kind: ModelErrorKind;
  message: string;
}

export interface ModelApi {
  stream(
    request: ModelRequest,
    options?: { signal?: AbortSignal },
  ): AsyncIterable<ModelStreamEvent> | Promise<AsyncIterable<ModelStreamEvent>>;
}

export type JsonPrimitive = null | boolean | number | string;
export type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };

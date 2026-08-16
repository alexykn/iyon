import type { ModelStreamEvent, StopReason, Usage } from "iyon:api";
import type { JsonValue } from "@iyon/sdk";

export interface ToolCallBuffer { id: string; name: string; arguments: string; started: boolean; }
export interface OpenRouterStreamState {
  readonly toolCalls: Map<number, ToolCallBuffer>;
  stopReason: StopReason;
  anyToolCall: boolean;
}

export function createStreamState(): OpenRouterStreamState {
  return { toolCalls: new Map(), stopReason: "stop", anyToolCall: false };
}

export function normalizeChunk(data: JsonValue, state: OpenRouterStreamState): ModelStreamEvent[] {
  if (!data || typeof data !== "object" || Array.isArray(data)) throw providerError("invalid chat chunk shape");
  const object = data as Record<string, JsonValue>;
  if (typeof object.error === "string") throw providerError(object.error);
  const events: ModelStreamEvent[] = [];
  const choices = Array.isArray(object.choices) ? object.choices : [];
  const choice = choices[0];
  if (choice && typeof choice === "object" && !Array.isArray(choice)) {
    const value = choice as Record<string, JsonValue>;
    const delta = value.delta;
    if (delta && typeof delta === "object" && !Array.isArray(delta)) {
      const deltaValue = delta as Record<string, JsonValue>;
      const calls = Array.isArray(deltaValue.tool_calls) ? deltaValue.tool_calls : [];
      for (const item of calls) {
        if (!item || typeof item !== "object" || Array.isArray(item)) continue;
        const call = item as Record<string, JsonValue>;
        const index = typeof call.index === "number" ? call.index : 0;
        const entry = state.toolCalls.get(index) ?? { id: "", name: "", arguments: "", started: false };
        if (typeof call.id === "string" && !entry.id) entry.id = call.id;
        const fn = call.function;
        if (fn && typeof fn === "object" && !Array.isArray(fn)) {
          const functionValue = fn as Record<string, JsonValue>;
          if (typeof functionValue.name === "string" && !entry.name) entry.name = functionValue.name;
        }
        if (!entry.started && entry.id && entry.name) {
          entry.started = true;
          state.anyToolCall = true;
          events.push({ type: "toolCallStart", contentIndex: index, id: entry.id, name: entry.name });
        }
        const fnValue = fn && typeof fn === "object" && !Array.isArray(fn) ? fn as Record<string, JsonValue> : undefined;
        if (typeof fnValue?.arguments === "string" && fnValue.arguments) {
          entry.arguments += fnValue.arguments;
          events.push({ type: "toolCallDelta", contentIndex: index, id: entry.id || undefined, name: entry.name || undefined, argumentsDelta: fnValue.arguments });
        }
        state.toolCalls.set(index, entry);
      }
      if (typeof deltaValue.reasoning === "string" && deltaValue.reasoning) events.push({ type: "thinkingDelta", contentIndex: 0, delta: deltaValue.reasoning });
      if (typeof deltaValue.content === "string" && deltaValue.content) events.push({ type: "textDelta", contentIndex: 0, delta: deltaValue.content });
    }
    if (typeof value.finish_reason === "string" && value.finish_reason) state.stopReason = mapFinishReason(value.finish_reason);
  }
  const usage = object.usage;
  if (usage && typeof usage === "object" && !Array.isArray(usage)) {
    const value = usage as Record<string, JsonValue>;
    const details = value.prompt_tokens_details;
    const cached = details && typeof details === "object" && !Array.isArray(details) && typeof (details as Record<string, JsonValue>).cached_tokens === "number" ? (details as Record<string, JsonValue>).cached_tokens as number : 0;
    const input = typeof value.prompt_tokens === "number" ? value.prompt_tokens : 0;
    const output = typeof value.completion_tokens === "number" ? value.completion_tokens : 0;
    const normalized: Usage = { inputTokens: Math.max(0, input - cached), outputTokens: output, cacheReadTokens: cached, cacheWriteTokens: 0 };
    events.push({ type: "usage", usage: normalized });
  }
  return events;
}

export function flushToolCalls(state: OpenRouterStreamState): ModelStreamEvent[] {
  return [...state.toolCalls.entries()].filter(([, call]) => call.started).map(([contentIndex, call]) => ({
    type: "toolCallEnd",
    contentIndex,
    id: call.id,
    name: call.name,
    arguments: parseArguments(call.arguments),
  }));
}

function parseArguments(value: string): JsonValue { try { return JSON.parse(value) as JsonValue; } catch { return {}; } }
function mapFinishReason(reason: string): StopReason {
  if (reason === "tool_calls") return "toolUse";
  if (reason === "length") return "length";
  if (reason === "content_filter" || reason === "error") return "error";
  return "stop";
}
function providerError(message: string): Error & { readonly kind: "provider" } {
  return Object.assign(new Error(message), { kind: "provider" as const });
}

import type { ModelStreamEvent, StopReason, Usage } from "iyon:api";
import type { JsonValue } from "@iyon/sdk";

interface ToolCall { id?: string; name?: string; arguments: string; }
export interface CodexStreamState { text: string[]; thinking: string[]; tools: ToolCall[]; sawToolCall: boolean; stopReason: StopReason; }
export function createStreamState(): CodexStreamState { return { text: [], thinking: [], tools: [], sawToolCall: false, stopReason: "stop" }; }

export function normalizeEvent(event: JsonValue, state: CodexStreamState): ModelStreamEvent[] {
  if (!event || typeof event !== "object" || Array.isArray(event)) throw providerError("invalid codex event shape");
  const value = event as Record<string, JsonValue>;
  const kind = typeof value.type === "string" ? value.type : "";
  const events: ModelStreamEvent[] = [];
  if (kind === "response.output_item.added") {
    const item = value.item;
    if (item && typeof item === "object" && !Array.isArray(item)) {
      const itemValue = item as Record<string, JsonValue>;
      if (itemValue.type === "message") { state.text.push(""); events.push({ type: "textStart", contentIndex: state.text.length - 1 }); }
      if (itemValue.type === "reasoning") { state.thinking.push(""); events.push({ type: "thinkingStart", contentIndex: state.thinking.length - 1 }); }
      if (itemValue.type === "function_call") {
        state.tools.push({ id: stringValue(itemValue.call_id), name: stringValue(itemValue.name), arguments: stringValue(itemValue.arguments) ?? "" });
        const index = state.tools.length - 1;
        const tool = state.tools[index];
        events.push({ type: "toolCallStart", contentIndex: index, id: tool.id, name: tool.name });
      }
    }
  }
  if (kind === "response.output_text.delta" || kind === "response.refusal.delta") {
    const delta = stringValue(value.delta);
    if (delta && state.text.length > 0) { state.text[state.text.length - 1] += delta; events.push({ type: "textDelta", contentIndex: state.text.length - 1, delta }); }
  }
  if (kind === "response.reasoning_summary_text.delta") {
    const delta = stringValue(value.delta);
    if (delta && state.thinking.length > 0) { state.thinking[state.thinking.length - 1] += delta; events.push({ type: "thinkingDelta", contentIndex: state.thinking.length - 1, delta }); }
  }
  if (kind === "response.reasoning_summary_part.done" && state.thinking.length > 0) { state.thinking[state.thinking.length - 1] += "\n\n"; events.push({ type: "thinkingDelta", contentIndex: state.thinking.length - 1, delta: "\n\n" }); }
  if (kind === "response.function_call_arguments.delta") {
    const delta = stringValue(value.delta);
    if (delta && state.tools.length > 0) { const tool = state.tools.at(-1)!; tool.arguments += delta; events.push({ type: "toolCallDelta", contentIndex: state.tools.length - 1, id: tool.id, name: tool.name, argumentsDelta: delta }); }
  }
  if (kind === "response.function_call_arguments.done" && state.tools.length > 0) {
    const complete = stringValue(value.arguments) ?? "";
    const tool = state.tools.at(-1)!;
    if (complete.startsWith(tool.arguments)) {
      const delta = complete.slice(tool.arguments.length);
      if (delta) events.push({ type: "toolCallDelta", contentIndex: state.tools.length - 1, id: tool.id, name: tool.name, argumentsDelta: delta });
    }
    tool.arguments = complete;
  }
  if (kind === "response.output_item.done") {
    const item = value.item;
    if (item && typeof item === "object" && !Array.isArray(item)) {
      const type = (item as Record<string, JsonValue>).type;
      if (type === "message" && state.text.length > 0) events.push({ type: "textEnd", contentIndex: state.text.length - 1, text: state.text.at(-1)! });
      if (type === "reasoning" && state.thinking.length > 0) events.push({ type: "thinkingEnd", contentIndex: state.thinking.length - 1, text: state.thinking.at(-1)! });
      if (type === "function_call" && state.tools.length > 0) {
        const tool = state.tools.at(-1)!;
        if (tool.id && tool.name) { state.sawToolCall = true; events.push({ type: "toolCallEnd", contentIndex: state.tools.length - 1, id: tool.id, name: tool.name, arguments: parseArguments(tool.arguments) }); }
      }
    }
  }
  if (kind === "response.completed" || kind === "response.done" || kind === "response.incomplete") {
    const response = value.response;
    if (response && typeof response === "object" && !Array.isArray(response)) {
      const responseValue = response as Record<string, JsonValue>;
      const usage = responseValue.usage;
      if (usage && typeof usage === "object" && !Array.isArray(usage)) {
        const usageValue = usage as Record<string, JsonValue>;
        const details = usageValue.input_tokens_details;
        const cached = details && typeof details === "object" && !Array.isArray(details) && typeof (details as Record<string, JsonValue>).cached_tokens === "number" ? (details as Record<string, JsonValue>).cached_tokens as number : 0;
        const input = numberValue(usageValue.input_tokens) ?? 0;
        const output = numberValue(usageValue.output_tokens) ?? 0;
        const normalized: Usage = { inputTokens: Math.max(0, input - cached), outputTokens: output, cacheReadTokens: cached, cacheWriteTokens: 0 };
        events.push({ type: "usage", usage: normalized });
      }
      state.stopReason = mapStatus(stringValue(responseValue.status));
    }
  }
  if (kind === "response.failed") throw providerError(stringValue((value.response as Record<string, JsonValue> | undefined)?.error && ((value.response as Record<string, JsonValue>).error as Record<string, JsonValue>).message) ?? "codex response failed");
  if (kind === "error") throw providerError(stringValue(value.message) ?? "provider error");
  return events;
}

function stringValue(value: JsonValue | undefined): string | undefined { return typeof value === "string" ? value : undefined; }
function numberValue(value: JsonValue | undefined): number | undefined { return typeof value === "number" ? value : undefined; }
function parseArguments(value: string): JsonValue { try { return JSON.parse(value) as JsonValue; } catch { return {}; } }
function mapStatus(status?: string): StopReason { return status === "incomplete" ? "length" : status === "failed" ? "error" : status === "cancelled" ? "aborted" : "stop"; }
function providerError(message: string): Error & { readonly kind: "provider" } { return Object.assign(new Error(message), { kind: "provider" as const }); }

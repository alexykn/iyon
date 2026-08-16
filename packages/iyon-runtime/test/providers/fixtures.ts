import { expect } from "bun:test";
import type { ModelStreamEvent } from "@iyon/sdk";

export const providerOracleCases = {
  mockTextStream: "mock text stream",
  openRouterTextReasoningToolUsage: "openrouter text/reasoning/tool/usage",
  openRouterError: "openrouter error",
  codexMessageReasoningFunctionCall: "codex message/reasoning/function-call",
  codexCompletionError: "codex completion/error",
} as const;

export async function collectEvents(stream: AsyncIterable<ModelStreamEvent>): Promise<ModelStreamEvent[]> {
  const events: ModelStreamEvent[] = [];
  for await (const event of stream) events.push(event);
  return events;
}

export function expectEventSequence(actual: readonly ModelStreamEvent[], expected: readonly ModelStreamEvent[]): void {
  expect(redact(actual)).toEqual(redact(expected));
}

export function redact<T>(value: T): T {
  if (typeof value === "string" && /(api[-_ ]?key|access|refresh|token|secret)/i.test(value)) return "[REDACTED]" as T;
  if (Array.isArray(value)) return value.map(redact) as T;
  if (value && typeof value === "object") {
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [
      key,
      /(key|token|secret|credential|authorization)/i.test(key) ? "[REDACTED]" : redact(item),
    ])) as T;
  }
  return value;
}

import type { ModelRequest, ModelStreamEvent } from "iyon:api";

export function textTurn(text: string, stopReason: "stop" | "length" = "stop"): readonly ModelStreamEvent[] {
  return [
    { type: "started" },
    { type: "textDelta", contentIndex: 0, delta: text },
    { type: "done", stopReason },
  ];
}

export function emptyRequest(): ModelRequest {
  return { messages: [], tools: [], params: {}, metadata: {} };
}

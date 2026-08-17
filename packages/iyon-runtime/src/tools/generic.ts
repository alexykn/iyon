import { Style, View } from "../tui/index.ts";
import type { ToolCall, ToolResult } from "./contract.ts";

export function renderGenericCall(call: ToolCall): View {
  const status = call.state;
  const lines = [`tool ${call.name} — ${status}`];
  if (call.showArgPreview) lines.push(...jsonLines(call.arguments));
  return View.vertical(lines.map((line) => View.text(line).fillWidth())).fillWidth() as unknown as View;
}

export function renderGenericResult(result: ToolResult): View {
  const title = result.isError ? "failed" : "result";
  const text = result.text ?? result.content.filter((block) => block.type === "text").map((block) => block.text).join("");
  const style = Style.new().foreground(`theme:${result.isError ? "text.error" : "text.muted"}`);
  const body = text.split(/\r?\n/u).map((line) => View.text(line).style(style).fillWidth());
  return View.vertical([View.text(`${result.toolName ?? "tool"} ${title}`).style(style).fillWidth(), ...body]).fillWidth() as unknown as View;
}

export const genericRenderer = {
  renderCall: renderGenericCall,
  renderResult: renderGenericResult,
};

function jsonLines(value: unknown): string[] {
  try {
    return JSON.stringify(value, null, 2)?.split("\n") ?? [String(value)];
  } catch {
    return [String(value)];
  }
}

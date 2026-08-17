import { Style, View } from "../tui/index.ts";
import type { ToolCall, ToolResult } from "./contract.ts";

export function renderGenericCall(call: ToolCall): View {
  const bulletStyle = Style.new().theme(call.state === "failed" || call.state === "cancelled" ? "tool.error" : call.state === "prepared" || call.state === "finished" ? "tool.finished" : "tool.running");
  const label = call.state === "prepared" ? "ready" : call.state === "pendingApproval" ? "waiting for approval" : call.state;
  return View.hanging(View.text("● ").style(call.pulse ? bulletStyle.dim() : bulletStyle), View.text("  "), View.text(`${call.name || "tool"} — ${label}`).fillWidth()).fillWidth() as unknown as View;
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

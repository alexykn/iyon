import { View } from "@iyon/tui";
import type { ToolCall, ToolResult } from "@iyon/sdk";
import { resultStyle, resultText, statusLabel, toolCallLine, toolResultLine } from "@iyon/plugins";

export function renderReadCall(call: ToolCall<{ path: string; offset?: number; limit?: number }>): View {
  if (call.arguments === undefined) return toolCallLine(`read — ${statusLabel(call.state)}`, call.state, call.pulse);
  const { path, offset, limit } = call.arguments;
  const suffix = offset === undefined ? "" : limit === undefined ? `:${offset}` : `:${offset}-${offset + Math.max(0, limit - 1)}`;
  return toolCallLine(`read ${path}${suffix} — ${statusLabel(call.state)}`, call.state, call.pulse);
}

export function renderReadResult(result: ToolResult): View {
  const style = resultStyle(result.isError);
  // Keep multiline content in one text node; the terminal renderer still
  // wraps it into rows without creating one retained view per source line.
  return View.vertical([toolResultLine(result.isError ? "read failed" : "read result", style), toolResultLine(resultText(result), style)]).fillWidth();
}

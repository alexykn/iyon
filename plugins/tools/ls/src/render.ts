import { View } from "iyon:tui";
import type { ToolCall, ToolResult } from "@iyon/sdk";
import { resultLines, resultStyle, resultText, statusLabel, toolCallLine, toolResultLine } from "@iyon/plugins";

export function renderLsCall(call: ToolCall<{ path?: string }>): View {
  return toolCallLine(`ls ${call.arguments.path ?? "."} — ${statusLabel(call.state)}`, call.state, call.pulse) as unknown as View;
}

export function renderLsResult(result: ToolResult): View {
  const style = resultStyle(result.isError);
  return View.vertical([toolResultLine(result.isError ? "ls failed" : "ls result", style), ...resultLines(resultText(result), style)]).fillWidth() as unknown as View;
}

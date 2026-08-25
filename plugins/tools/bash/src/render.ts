import { View } from "iyon:tui";
import type { ToolCall, ToolResult } from "@iyon/sdk";
import { resultStyle, resultText, statusLabel, toolCallLine, toolResultLine } from "@iyon/plugins";

export function renderBashCall(call: ToolCall<{ command: string }>): View {
  const label = call.arguments === undefined
    ? `${call.name} — ${statusLabel(call.state)}`
    : `$ ${call.arguments.command} — ${statusLabel(call.state)}`;
  return toolCallLine(label, call.state, call.pulse) as unknown as View;
}

export function renderBashResult(result: ToolResult): View {
  const style = resultStyle(result.isError);
  const text = resultText(result);
  // Use a single View.text block instead of per-line Views (resultLines)
  // so large outputs don't create thousands of DAG nodes that freeze the
  // TS↔Rust bridge sync. The text is clamped to 16 rows downstream.
  const children = [
    toolResultLine(result.isError ? "bash failed" : "bash result", style),
    View.text(text).fillWidth().style(style),
  ];
  const fullOutputPath = typeof result.details === "object" && result.details !== null && typeof (result.details as { fullOutputPath?: unknown }).fullOutputPath === "string" ? (result.details as { fullOutputPath: string }).fullOutputPath : undefined;
  if (fullOutputPath) children.push(toolResultLine(`[Full output: ${fullOutputPath}]`, style.theme("text.warning")));
  return View.vertical(children).fillWidth() as unknown as View;
}

import { StyleRef, View } from "@iyon/tui";
import type { ToolCall, ToolResult } from "@iyon/sdk";
import { resultStyle, resultText, statusLabel, toolCallLine, toolResultLine } from "@iyon/plugins";

export function renderBashCall(call: ToolCall<{ command: string }>): View {
  const label = call.arguments === undefined
    ? `${call.name} — ${statusLabel(call.state)}`
    : `$ ${call.arguments.command} — ${statusLabel(call.state)}`;
  return toolCallLine(label, call.state, call.pulse);
}

export function renderBashResult(result: ToolResult): View {
  const style = resultStyle(result.isError);
  const text = resultText(result);
  // Use a single View.text block instead of per-line Views (resultLines)
  // so large outputs don't create thousands of DAG nodes that freeze the
  // TS↔Rust bridge sync. The scroll pane handles line wrapping internally.
  // ⚠️ If you need per-line styling, add it to the scroll pane renderer on
  // the Rust side — do NOT split into per-line View nodes here.
  const children = [
    toolResultLine(result.isError ? "bash failed" : "bash result", style),
    // Keep final output on the same two-column hanging indent as live
    // updates; multiline text remains one retained node.
    toolResultLine(text, style),
  ];
  const fullOutputPath = typeof result.details === "object" && result.details !== null && typeof (result.details as { fullOutputPath?: unknown }).fullOutputPath === "string" ? (result.details as { fullOutputPath: string }).fullOutputPath : undefined;
  if (fullOutputPath) children.push(toolResultLine(`[Full output: ${fullOutputPath}]`, StyleRef.theme("text.warning", style)));
  return View.vertical(children).fillWidth();
}

import { View } from "iyon:tui";
import type { ToolCall, ToolResult } from "@iyon/sdk";
import { renderDiff } from "@iyon/plugins";
export function renderEditCall(call: ToolCall<{ path: string }>): View { return View.text(`edit ${call.arguments.path} — ${call.state}`).fillWidth() as unknown as View; }
export function renderEditResult(result: ToolResult): View { const diff = renderDiff(result.details); const summary = View.text(result.isError ? "edit failed" : text(result)).fillWidth(); return (diff ? View.vertical([summary, diff]) : summary) as unknown as View; }
function text(result: ToolResult): string { return result.text ?? result.content.filter((block) => block.type === "text").map((block) => block.text).join(""); }

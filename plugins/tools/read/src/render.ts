import { View } from "iyon:tui";
import type { ToolCall, ToolResult } from "@iyon/sdk";

export function renderReadCall(call: ToolCall<{ path: string }>): View { return View.text(`read ${call.arguments.path} — ${call.state}`).fillWidth() as unknown as View; }
export function renderReadResult(result: ToolResult): View { return View.text(result.isError ? `read failed: ${result.text ?? text(result)}` : text(result)).fillWidth() as unknown as View; }
function text(result: ToolResult): string { return result.text ?? result.content.filter((block) => block.type === "text").map((block) => block.text).join(""); }

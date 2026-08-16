import { View } from "iyon:tui";
import type { ToolCall, ToolResult } from "@iyon/sdk";
export function renderBashCall(call: ToolCall<{ command: string }>): View { return View.text(`bash ${call.arguments.command} — ${call.state}`).fillWidth() as unknown as View; }
export function renderBashResult(result: ToolResult): View { return View.vertical([View.text(result.isError ? "bash failed" : "bash").fillWidth(), ...text(result).split("\n").map((line) => View.text(line).fillWidth())]).fillWidth() as unknown as View; }
function text(result: ToolResult): string { return result.text ?? result.content.filter((block) => block.type === "text").map((block) => block.text).join(""); }

import { View } from "iyon:tui";
import type { ToolCall, ToolResult } from "@iyon/sdk";
export function renderGrepCall(call: ToolCall<{ pattern: string }>): View { return View.text(`grep ${call.arguments.pattern} — ${call.state}`).fillWidth() as unknown as View; }
export function renderGrepResult(result: ToolResult): View { return View.vertical([View.text(result.isError ? "grep failed" : "grep").fillWidth(), ...text(result).split("\n").map((line) => View.text(line).fillWidth())]).fillWidth() as unknown as View; }
function text(result: ToolResult): string { return result.text ?? result.content.filter((block) => block.type === "text").map((block) => block.text).join(""); }

import { View } from "iyon:tui";
import type { ToolCall, ToolResult } from "@iyon/sdk";
export function renderLsCall(call: ToolCall<{ path?: string }>): View { return View.text(`ls ${call.arguments.path ?? "."} — ${call.state}`).fillWidth() as unknown as View; }
export function renderLsResult(result: ToolResult): View { return View.vertical([View.text(result.isError ? "ls failed" : "ls").fillWidth(), ...text(result).split("\n").map((line) => View.text(line).fillWidth())]).fillWidth() as unknown as View; }
function text(result: ToolResult): string { return result.text ?? result.content.filter((block) => block.type === "text").map((block) => block.text).join(""); }

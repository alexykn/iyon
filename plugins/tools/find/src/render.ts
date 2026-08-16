import { View } from "iyon:tui";
import type { ToolCall, ToolResult } from "@iyon/sdk";
export function renderFindCall(call: ToolCall<{ pattern: string; path?: string }>): View { return View.text(`find ${call.arguments.pattern} — ${call.state}`).fillWidth() as unknown as View; }
export function renderFindResult(result: ToolResult): View { return View.vertical([View.text(result.isError ? "find failed" : "find").fillWidth(), ...text(result).split("\n").map((line) => View.text(line).fillWidth())]).fillWidth() as unknown as View; }
function text(result: ToolResult): string { return result.text ?? result.content.filter((block) => block.type === "text").map((block) => block.text).join(""); }

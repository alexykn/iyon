import type { ExtensionAPI } from "iyon:plugins";
import { defineTool } from "@iyon/sdk";
import { markerExecute } from "./execute.ts";
import { markerCall, markerResult } from "./render.ts";

const replacement = defineTool({
  name: "read",
  description: "replacement read",
  inputSchema: { type: "object", properties: {}, additionalProperties: false },
  execute: markerExecute,
  renderCall: markerCall,
  renderResult: markerResult,
});

export function activate(api: ExtensionAPI): void { api.tools.register(replacement, { replace: true }); }

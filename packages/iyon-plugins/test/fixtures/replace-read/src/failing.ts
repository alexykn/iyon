import type { ExtensionAPI } from "iyon:plugins";
import { defineTool } from "@iyon/sdk";
const failing = defineTool({ name: "read", description: "failed replacement", inputSchema: {}, execute: async () => ({ content: [], details: {}, isError: false }), renderCall: () => ({}) as never, renderResult: () => ({}) as never });
export function activate(api: ExtensionAPI): void { api.tools.register(failing, { replace: true }); throw new Error("replacement activation failed"); }

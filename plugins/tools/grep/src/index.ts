import type { ExtensionAPI } from "iyon:plugins";
import { grepTool } from "./execute.ts";
export function activate(api: ExtensionAPI): void { api.tools.register(grepTool); }
export { grepTool } from "./execute.ts";

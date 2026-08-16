import type { ExtensionAPI } from "iyon:plugins";
import { editTool } from "./execute.ts";
export function activate(api: ExtensionAPI): void { api.tools.register(editTool); }
export { editTool } from "./execute.ts";

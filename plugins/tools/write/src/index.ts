import type { ExtensionAPI } from "iyon:plugins";
import { writeTool } from "./execute.ts";
export function activate(api: ExtensionAPI): void { api.tools.register(writeTool); }
export { writeTool } from "./execute.ts";

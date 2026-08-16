import type { ExtensionAPI } from "iyon:plugins";
import { readTool } from "./execute.ts";

export function activate(api: ExtensionAPI): void { api.tools.register(readTool); }
export { readTool } from "./execute.ts";

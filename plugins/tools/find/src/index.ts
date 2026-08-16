import type { ExtensionAPI } from "iyon:plugins";
import { findTool } from "./execute.ts";
export function activate(api: ExtensionAPI): void { api.tools.register(findTool); }
export { findTool } from "./execute.ts";

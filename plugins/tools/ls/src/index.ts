import type { ExtensionAPI } from "iyon:plugins";
import { lsTool } from "./execute.ts";
export function activate(api: ExtensionAPI): void { api.tools.register(lsTool); }
export { lsTool } from "./execute.ts";

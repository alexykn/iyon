import type { ExtensionAPI } from "iyon:plugins";
import { bashTool } from "./execute.ts";
export function activate(api: ExtensionAPI): void { api.tools.register(bashTool); }
export { bashTool } from "./execute.ts";
export { bashApprovalPolicy, bashCommandUsesSudo } from "./policy.ts";

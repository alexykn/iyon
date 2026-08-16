import type { ExtensionAPI } from "iyon:plugins";
export async function activate(api: ExtensionAPI) { await Promise.resolve(); api.tools.register({ id: "fixture.async" }); }

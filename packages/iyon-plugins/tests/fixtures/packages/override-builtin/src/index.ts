import type { ExtensionAPI } from "iyon:plugins";
export function activate(api: ExtensionAPI) { api.tools.register({ id: "fixture.override", value: "extension" }, { replace: true }); }

import type { ExtensionAPI } from "iyon:plugins";
export function activate(api: ExtensionAPI) {
  api.tools.register({ id: "fixture.failure.one" });
  api.tools.register({ id: "fixture.failure.two" });
  api.on("registration", () => undefined);
  throw new Error("fixture activation failed");
}

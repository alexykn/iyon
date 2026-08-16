import type { ExtensionAPI } from "iyon:plugins";
export function activate(api: ExtensionAPI) {
  api.tools.register({ id: "fixture.multiple.tool" });
  api.providers.register({ id: "fixture.multiple.provider" });
  api.agents.register({ id: "fixture.multiple.agent" });
  api.apps.register({ id: "fixture.multiple.app", create: () => ({}) });
  api.commands.register({ id: "fixture.multiple.command" });
  api.shortcuts.register({ id: "fixture.multiple.shortcut" });
}

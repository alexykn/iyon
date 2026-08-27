import type { ExtensionAPI } from "iyon:plugins";
import { Scene } from "@iyon/tui";
export function activate(api: ExtensionAPI) { api.scene.compose({ id: "fixture.scene.compose", compose: (scene) => new Scene(scene.body, scene.history) }); }

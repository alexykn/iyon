import { dependencyValue } from "fixture-dependency-package";
import type { ExtensionAPI } from "iyon:plugins";
export function activate(api: ExtensionAPI) { api.tools.register({ id: `fixture.dependency.${dependencyValue}` }); }

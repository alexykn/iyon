import { installIyonVirtualModules } from "@iyon/runtime";
import { runProduction } from "./main.ts";
import { installBundledExtensionModules } from "./bundled-extensions.ts";

installIyonVirtualModules();
installBundledExtensionModules();
const exitCode = await runProduction();
if (exitCode !== 0) process.exitCode = exitCode;

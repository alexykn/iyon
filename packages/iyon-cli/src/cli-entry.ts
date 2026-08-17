import { installIyonVirtualModules } from "@iyon/runtime";
import { runProduction } from "./main.ts";
import { installBundledExtensionModules } from "./bundled-extensions.ts";
import { requestProcessSignal } from "./runner.ts";

installIyonVirtualModules();
installBundledExtensionModules();
const handleSignal = () => { void requestProcessSignal(); };
process.on("SIGINT", handleSignal);
process.on("SIGTERM", handleSignal);
const exitCode = await runProduction();
process.removeListener("SIGINT", handleSignal);
process.removeListener("SIGTERM", handleSignal);
if (exitCode !== 0) process.exitCode = exitCode;

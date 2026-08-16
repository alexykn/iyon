import { installIyonVirtualModules } from "@iyon/runtime";

installIyonVirtualModules();

const { runSmokeCommand } = await import("./smoke-command.ts");

try {
  const result = await runSmokeCommand();
  console.log(JSON.stringify(result));
} catch (error) {
  const message = error instanceof Error ? error.message : String(error);
  console.error(JSON.stringify({ ok: false, error: message }));
  process.exitCode = 1;
}

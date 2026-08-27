import { installIyonVirtualModules } from "@iyon/runtime";

async function main(): Promise<void> {
  installIyonVirtualModules();
  try {
    const { runSmokeCommand } = await import("./smoke-command.ts");
    const result = await runSmokeCommand();
    console.log(JSON.stringify(result));
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    console.error(JSON.stringify({ ok: false, error: message }));
    process.exitCode = 1;
  }
}

void main();

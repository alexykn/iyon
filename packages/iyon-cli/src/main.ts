import { parseArgs, type CliCommand } from "./args.ts";
import { runAuth } from "./auth.ts";
import { runBootstrap, type BootstrapStages } from "./bootstrap.ts";
import { createProductionStages } from "./bootstrap.ts";

export interface CliDependencies { readonly stages: BootstrapStages; readonly auth?: (command: Extract<CliCommand, { type: "auth" }>["command"]) => Promise<unknown>; readonly argv?: readonly string[]; }

export async function runCli(dependencies: CliDependencies): Promise<unknown> {
  const command = parseArgs(dependencies.argv);
  if (command.type === "help") { console.log("Usage: iyon [run|auth login|auth logout|auth status]"); return undefined; }
  if (command.type === "auth") {
    if (dependencies.auth) return dependencies.auth(command.command);
    return runBootstrap(command, dependencies.stages);
  }
  return runBootstrap(command, dependencies.stages);
}

export async function main(dependencies: CliDependencies): Promise<number> {
  try { await runCli(dependencies); return 0; } catch (error) { console.error(error instanceof Error ? error.message : String(error)); return 1; }
}

export async function runProduction(argv: readonly string[] = process.argv.slice(2)): Promise<number> {
  const command = parseArgs(argv);
  return main({ argv, stages: createProductionStages({ authOnly: command.type === "auth" }) });
}

import type { CliCommand } from "./args.ts";
import { CleanupStack } from "./cleanup.ts";

export interface BootstrapContext { readonly config: unknown; readonly runtime: unknown; readonly packages: unknown; readonly provider: unknown; readonly agent: unknown; readonly app: unknown; }
export interface BootstrapStages {
  loadConfig(): Promise<unknown>;
  initializeNative(): Promise<unknown>;
  initializeVirtualModules(): Promise<unknown>;
  discoverPackages(): Promise<unknown>;
  activateExtensions(packages: unknown): Promise<unknown>;
  selectProvider(activated: unknown, config: unknown): Promise<unknown>;
  selectAgent(activated: unknown, provider: unknown, config: unknown): Promise<unknown>;
  selectApp(activated: unknown, agent: unknown, provider: unknown, config: unknown): Promise<unknown>;
  runApp(app: unknown, context: BootstrapContext): Promise<unknown>;
  cleanup?(context: BootstrapContext): Promise<void>;
}

export interface BootstrapResult { readonly command: CliCommand; readonly result?: unknown; readonly context?: BootstrapContext; }

export async function runBootstrap(command: CliCommand, stages: BootstrapStages): Promise<BootstrapResult> {
  if (command.type === "auth") return { command };
  const cleanup = new CleanupStack();
  let context: BootstrapContext | undefined;
  try {
    const config = await stages.loadConfig();
    const runtime = await stages.initializeNative();
    cleanup.use(async () => { if (stages.cleanup && context) await stages.cleanup(context); });
    await stages.initializeVirtualModules();
    const packages = await stages.discoverPackages();
    const activated = await stages.activateExtensions(packages);
    const provider = await stages.selectProvider(activated, config);
    const agent = await stages.selectAgent(activated, provider, config);
    const app = await stages.selectApp(activated, agent, provider, config);
    context = { config, runtime, packages: activated, provider, agent, app };
    const result = await stages.runApp(app, context);
    return { command, result, context };
  } finally { await cleanup.close(); }
}

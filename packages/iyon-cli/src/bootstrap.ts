import type { CliCommand } from "./args.ts";
import { CleanupStack } from "./cleanup.ts";
import { installIyonVirtualModules, AgentSession, selectProvider } from "@iyon/runtime";
import { discoverPackages, PackageLoader, selectApp } from "@iyon/plugins";
import { selectIyonAgent } from "./selection.ts";
import { runSelectedApp, type RunnableAgent, type RunnableApp } from "./runner.ts";

const bundledRoots = [
  new URL("../../../plugins/app/iyon/", import.meta.url).pathname,
  new URL("../../../plugins/agents/iyon/", import.meta.url).pathname,
  new URL("../../../plugins/providers/mock/", import.meta.url).pathname,
  new URL("../../../plugins/providers/openrouter/", import.meta.url).pathname,
  new URL("../../../plugins/providers/openai-codex/", import.meta.url).pathname,
  ...["bash", "read", "write", "edit", "grep", "find", "ls"].map((name) => new URL(`../../../plugins/tools/${name}/`, import.meta.url).pathname),
] as const;

export function createProductionStages(): BootstrapStages {
  let loader: PackageLoader | undefined;
  let session: AgentSession | undefined;
  return {
    async loadConfig() { return { env: { ...process.env } }; },
    async initializeNative() { return { native: true }; },
    async initializeVirtualModules() { installIyonVirtualModules(); },
    async discoverPackages() { return discoverPackages({ bundled: bundledRoots }); },
    async activateExtensions(packages) {
      loader = new PackageLoader(); const result = await loader.loadAll(packages as Awaited<ReturnType<typeof discoverPackages>>);
      if (result.failures.length > 0) throw new AggregateError(result.failures.map((failure) => failure.error), "extension activation failed");
      return { loader, packages };
    },
    async selectProvider(activated, config) {
      const value = activated as { loader: PackageLoader }; return selectProvider({ registry: value.loader.registries.providers, env: (config as { env: NodeJS.ProcessEnv }).env, warn: (warning) => console.error(`warning: ${warning.message}`) });
    },
    async selectAgent(activated, provider) {
      const value = activated as { loader: PackageLoader }; session = new AgentSession();
      return selectIyonAgent(value.loader.registries.agents, { model: (provider as { model: unknown }).model, session, signal: new AbortController().signal, tools: value.loader.registries.tools }, "iyon");
    },
    async selectApp(activated, agent, provider) {
      const value = activated as { loader: PackageLoader }; const selection = (provider as { selection: { provider: string; model_id: string } }).selection;
      return selectApp(value.loader.registries.apps, { id: "iyon", context: { agent: (agent as { agent: unknown }).agent, core: session, model: { provider: selection.provider, modelId: selection.model_id } } });
    },
    async runApp(app, context) {
      const selectedAgent = context.agent as { agent: RunnableAgent }; const selectedApp = context.app as { app: RunnableApp }; if (!session) throw new Error("agent session was not initialized");
      return runSelectedApp({ app: selectedApp.app, agent: selectedAgent.agent, session });
    },
    async cleanup() { session?.close(); loader = undefined; session = undefined; },
  };
}

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

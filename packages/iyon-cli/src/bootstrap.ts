import { existsSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import type { CliCommand } from "./args.ts";
import type { ReasoningLevel } from "@iyon/sdk";
import { CleanupStack, isExpectedCleanupError } from "./cleanup.ts";
import { ApprovalBroker, AgentSession, installIyonVirtualModules, native, selectProvider } from "@iyon/runtime";
import { discoverPackages, PackageLoader, selectApp } from "@iyon/plugins";
import { selectIyonAgent } from "./selection.ts";
import { runSelectedApp, type RunnableAgent, type RunnableApp } from "./runner.ts";
import { runAuth as runProviderAuth } from "./auth.ts";

const sourceRoot = new URL("../../../", import.meta.url);
const workingRoot = pathToFileURL(`${process.cwd()}/`);
const repositoryRoot = existsSync(join(process.cwd(), "plugins")) ? workingRoot : sourceRoot;
const bundledRoots = [
  new URL("plugins/app/iyon/", repositoryRoot).pathname,
  new URL("plugins/agents/iyon/", repositoryRoot).pathname,
  new URL("plugins/providers/mock/", repositoryRoot).pathname,
  new URL("plugins/providers/openrouter/", repositoryRoot).pathname,
  new URL("plugins/providers/openai-codex/", repositoryRoot).pathname,
  ...["bash", "read", "write", "edit", "grep", "find", "ls"].map((name) => new URL(`plugins/tools/${name}/`, repositoryRoot).pathname),
] as const;

export function createProductionStages(options: { readonly authOnly?: boolean } = {}): BootstrapStages {
  let loader: PackageLoader | undefined;
  let session: AgentSession | undefined;
  let lifetime: AbortController | undefined;
  let approvals: ApprovalBroker | undefined;
  let selectedAgent: { cancel?: () => void; setReasoningEffort?: (level: ReasoningLevel) => void } | undefined;
  let reasoningEffort: ReasoningLevel = "medium";
  let core: {
    submitPrompt(text: string): Promise<void>;
    steer(text: string): Promise<void>;
    followUp(text: string): Promise<void>;
    submitTurn(text: string): Promise<void>;
    cancelActiveTurn(): void;
    setReasoningEffort(level: ReasoningLevel): void;
    cycleReasoningEffort(): void;
    approve(approvalId: number): void;
    reject(approvalId: number, reason?: string): void;
  } | undefined;
  const roots = options.authOnly ? bundledRoots.slice(2, 5) : bundledRoots;
  return {
    async loadConfig() { return { env: { ...process.env } }; },
    async initializeNative() {
      const version = native.nativeVersion();
      if (version !== "iyon-core-native/s5") throw new Error(`core native addon verification failed: ${version}`);
      return { version };
    },
    async initializeVirtualModules() { installIyonVirtualModules(); },
    async discoverPackages() { return discoverPackages({ bundled: roots }); },
    async activateExtensions(packages) {
      loader = new PackageLoader(); const result = await loader.loadAll(packages as Awaited<ReturnType<typeof discoverPackages>>);
      if (result.failures.length > 0) throw new AggregateError(result.failures.map((failure) => failure.error), `extension activation failed: ${result.failures.map((failure) => `${failure.packageId}/${failure.extensionId}: ${errorText(failure.error)}`).join("; ")}`);
      return { loader, packages };
    },
    async selectProvider(activated, config) {
      const value = activated as { loader: PackageLoader }; return selectProvider({ registry: value.loader.registries.providers, env: (config as { env: NodeJS.ProcessEnv }).env, warn: (warning) => console.error(`warning: ${warning.message}`) });
    },
    async runAuth(command, activated, config) {
      const value = activated as { loader: PackageLoader };
      return runProviderAuth(command, { registry: value.loader.registries.providers, env: (config as { env: NodeJS.ProcessEnv }).env, output: (line) => console.log(line) });
    },
    async selectAgent(activated, provider) {
      const value = activated as { loader: PackageLoader }; session = new AgentSession();
      lifetime = new AbortController();
      approvals = new ApprovalBroker();
      const submitPrompt = async (text: string): Promise<void> => {
        await session?.enqueue("prompt", text);
      };
      core = {
        submitPrompt,
        steer: async (text) => { await session?.enqueue("steer", text); },
        followUp: async (text) => { await session?.enqueue("followUp", text); },
        submitTurn: submitPrompt,
        cancelActiveTurn: () => { selectedAgent?.cancel?.(); },
        setReasoningEffort: (level) => {
          reasoningEffort = level;
          selectedAgent?.setReasoningEffort?.(level);
        },
        cycleReasoningEffort: () => {
          const levels: readonly ReasoningLevel[] = ["none", "minimal", "low", "medium", "high", "xhigh", "max"];
          const next = levels[(levels.indexOf(reasoningEffort) + 1) % levels.length] ?? "medium";
          core?.setReasoningEffort(next);
        },
        approve: (approvalId) => approvals?.approve(Number(approvalId)),
        reject: (approvalId, reason) => approvals?.reject(Number(approvalId), reason),
      };
      const selection = selectIyonAgent(value.loader.registries.agents, {
        model: (provider as { model: unknown }).model,
        session,
        signal: lifetime.signal,
        reasoningEffort,
        tools: value.loader.registries.tools,
        core,
        approval: (state: Parameters<NonNullable<import("@iyon/sdk").ToolContext["approval"]>>[0]) => approvals!.request(state, lifetime!.signal).then((decision) => decision.approved),
      }, "iyon");
      selectedAgent = selection.agent as typeof selectedAgent;
      return selection;
    },
    async selectApp(activated, agent, provider) {
      const value = activated as { loader: PackageLoader }; const selection = (provider as { selection: { provider: string; model_id: string } }).selection;
      if (!core || !session) throw new Error("agent core was not initialized");
      const tools = {
        get: (toolName: string) => value.loader.registries.tools.get(toolName) as { renderCall?: unknown; renderResult?: unknown } | undefined,
      };
      return selectApp(value.loader.registries.apps, { id: "iyon", context: { agent: (agent as { agent: unknown }).agent, core, model: { provider: selection.provider, modelId: selection.model_id }, tools } });
    },
    async runApp(app, context) {
      const selectedAgent = context.agent as { agent: RunnableAgent }; const selectedApp = context.app as { app: RunnableApp }; if (!session) throw new Error("agent session was not initialized");
      return runSelectedApp({ app: selectedApp.app, agent: selectedAgent.agent, session });
    },
    async cleanup() {
      const errors: Array<{ readonly label: string; readonly error: unknown }> = [];
      const attempt = async (label: string, operation: () => Promise<void> | void): Promise<void> => {
        try { await operation(); } catch (error) {
          if (!isExpectedCleanupError(error)) errors.push({ label, error });
        }
      };
      lifetime?.abort();
      await attempt("cancel agent", () => selectedAgent?.cancel?.());
      approvals?.cancelAll();
      try {
        session?.abort();
        session?.close();
      } catch (error) {
        if (!isExpectedCleanupError(error)) errors.push({ label: "close session", error });
      }
      if (loader) {
        for (const extension of [...loader.activeExtensions].reverse()) {
          await attempt(`unload ${extension.packageId}/${extension.extensionId}`, () => loader?.unload(extension.packageId, extension.extensionId));
        }
      }
      selectedAgent = undefined;
      approvals = undefined;
      session = undefined;
      loader = undefined;
      if (errors.length > 0) {
        const message = errors.map(({ label, error }) => `${label}: ${errorText(error)}`).join("; ");
        throw new AggregateError(errors.map(({ error }) => error), `cleanup failed: ${message}`);
      }
    },
  };
}

function errorText(error: unknown): string {
  if (!(error instanceof Error)) return String(error);
  return error.cause === undefined ? error.message : `${error.message} (${errorText(error.cause)})`;
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
  runAuth?(command: "login" | "logout" | "status", activated: unknown, config: unknown): Promise<unknown>;
  runApp(app: unknown, context: BootstrapContext): Promise<unknown>;
  cleanup?(context: BootstrapContext): Promise<void>;
}

export interface BootstrapResult { readonly command: CliCommand; readonly result?: unknown; readonly context?: BootstrapContext; }

export async function runBootstrap(command: CliCommand, stages: BootstrapStages): Promise<BootstrapResult> {
  const cleanup = new CleanupStack();
  let context: BootstrapContext | undefined;
  try {
    const config = await stages.loadConfig();
    const runtime = await stages.initializeNative();
    cleanup.use(async () => { if (stages.cleanup && context) await stages.cleanup(context); });
    await stages.initializeVirtualModules();
    const packages = await stages.discoverPackages();
    const activated = await stages.activateExtensions(packages);
    if (command.type === "auth") {
      context = { config, runtime, packages: activated, provider: undefined, agent: undefined, app: undefined };
      const result = stages.runAuth === undefined ? undefined : await stages.runAuth(command.command, activated, config);
      return { command, result, context };
    }
    const provider = await stages.selectProvider(activated, config);
    const agent = await stages.selectAgent(activated, provider, config);
    const app = await stages.selectApp(activated, agent, provider, config);
    context = { config, runtime, packages: activated, provider, agent, app };
    const result = await stages.runApp(app, context);
    return { command, result, context };
  } finally { await cleanup.close(); }
}

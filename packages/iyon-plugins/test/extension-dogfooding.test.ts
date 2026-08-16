import { describe, expect, test } from "bun:test";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { AgentSession, ApprovalBroker, installIyonVirtualModules, selectProvider } from "@iyon/runtime";
import { discoverPackages, PackageLoader, selectApp } from "../src/index.ts";
import { selectIyonAgent } from "../../iyon-cli/src/selection.ts";

installIyonVirtualModules();

const fixtureRoot = new URL("./fixtures/dogfood/", import.meta.url).pathname;
const bundledRoot = (name: string) => new URL(`../../../plugins/${name}/`, import.meta.url).pathname;

describe("extension dogfooding", () => {
  test("uses third-party contributions through the ordinary loader and selection seams", async () => {
    const server = Bun.serve({ port: 0, fetch: () => new Response("local-network") });
    const previousUrl = process.env.IYON_DOGFOOD_URL;
    const previousMarker = process.env.IYON_DOGFOOD_MARKER;
    process.env.IYON_DOGFOOD_URL = `http://127.0.0.1:${server.port}`;
    process.env.IYON_DOGFOOD_MARKER = "process-marker";
    try {
      const loader = new PackageLoader();
      const candidates = await discoverPackages({
        bundled: [
          bundledRoot("tools/read"),
          bundledRoot("app/iyon"),
          bundledRoot("agents/iyon"),
          bundledRoot("providers/mock"),
        ],
        project: [fixtureRoot],
      });
      const result = await loader.loadAll(candidates);
      expect(result.failures).toHaveLength(0);

      const read = loader.registries.tools.get("read") as unknown as { execute: () => Promise<{ content: readonly { text: string }[] }>; renderCall: () => { kind: string }; renderResult: () => { kind: string } };
      expect((await read.execute()).content[0]?.text).toBe("fixture execution");
      expect(read.renderCall().kind).toBe("view");
      expect(read.renderResult().kind).toBe("view");

      const provider = await selectProvider({ registry: loader.registries.providers, env: { IYON_PROVIDER: "fixture-provider", IYON_MODEL: "fixture-model" } });
      expect(provider.selection).toEqual({ provider: "fixture-provider", model_id: "fixture-model" });

      const session = new AgentSession();
      const broker = new ApprovalBroker();
      const agent = selectIyonAgent(loader.registries.agents, {
        marker: "session",
        session,
        tools: loader.registries.tools,
        approval: (state: Parameters<NonNullable<import("@iyon/sdk").ToolContext["approval"]>>[0]) => broker.request(state).then((decision) => decision.approved),
      }, "fixture-agent");
      expect(agent.id).toBe("fixture-agent");

      const app = await selectApp(loader.registries.apps, { id: "fixture-app", context: { agent: agent.agent } });
      expect(app.id).toBe("fixture-app");
      expect(app.source.packageId).toBe("fixture-dogfood");
      expect(await customAppCreated()).toBe(1);

      const scene = await loader.registries.scene.apply({ body: "base" }, { appId: app.id });
      expect(scene).toEqual({ body: "replaced:fixture-app:composed" });

      expect(await runtimeProbe()).toEqual({ npm: "function", file: true, process: "process-marker", network: "local-network" });
      const capturedContext = await selectedAgentContext() as { readonly approval?: unknown };
      expect(capturedContext).toMatchObject({ marker: "session", session, tools: loader.registries.tools });
      expect(typeof capturedContext.approval).toBe("function");

      session.close();
      await unload(loader);
    } finally {
      server.stop();
      restoreEnv("IYON_DOGFOOD_URL", previousUrl);
      restoreEnv("IYON_DOGFOOD_MARKER", previousMarker);
    }
  });

  test("does not hard-code bundled tool names in the app", async () => {
    const root = join(import.meta.dir, "../../../plugins/app/iyon/src");
    const files = ["app.ts", "actions.ts", "backend.ts", "contracts.ts", "state.ts", "tool-cards.ts", "view.ts"];
    const source = (await Promise.all(files.map((file) => readFile(join(root, file), "utf8")))).join("\n");
    for (const name of ["bash", "read", "write", "edit", "grep", "find", "ls"]) expect(source).not.toMatch(new RegExp(`(?:case|name|toolName)\\s*[:=]?\\s*["']${name}["']`));
  });
});

async function unload(loader: PackageLoader): Promise<void> {
  for (const extension of [...loader.activeExtensions].reverse()) await loader.unload(extension.packageId, extension.extensionId);
}

function restoreEnv(name: string, value: string | undefined): void {
  if (value === undefined) delete process.env[name];
  else process.env[name] = value;
}

async function runtimeProbe() {
  const module = await import("./fixtures/dogfood/src/index.ts");
  return module.runtimeProbe;
}

async function selectedAgentContext() {
  const module = await import("./fixtures/dogfood/src/index.ts");
  return module.selectedAgentContext;
}

async function customAppCreated() {
  const module = await import("./fixtures/dogfood/src/index.ts");
  return module.customAppCreated;
}

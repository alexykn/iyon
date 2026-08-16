import { describe, expect, test } from "bun:test";
import { installIyonVirtualModules } from "../src/virtual-modules.ts";
import { discoverPackages, PackageLoader } from "@iyon/plugins";
import { SessionDriver } from "../../../plugins/agents/iyon/test/support/session-driver.ts";
import { textTurn } from "../../../plugins/agents/iyon/test/support/fixtures.ts";
import { runIndependentSubagent } from "./fixtures/subagent.ts";
import type { CustomAgentContext } from "./fixtures/custom-agent.ts";

installIyonVirtualModules();

const fixtureRoot = new URL("./fixtures/", import.meta.url).pathname;

describe("T9 subagent proof", () => {
  test("agent_can_run_an_independent_subagent_session", async () => {
    const loader = await loadCustomLoader();
    const parent = await SessionDriver.create(801);
    const child = await SessionDriver.create(802);
    const parentContext: CustomAgentContext = { session: parent.session, model: parent.provider, signal: new AbortController().signal };
    parent.session.appendMessage({ role: "user", content: [{ type: "text", text: "parent" }] });
    child.enqueue(textTurn("child"));
    const contribution = loader.registries.agents.get("custom");
    if (!contribution?.create) throw new Error("custom agent was not registered");
    const result = await runIndependentSubagent(parentContext, (context) => contribution.create!(context) as { run(): Promise<never> }, child.provider);
    expect(result.childSession.snapshot().entries.some((entry) => entry.kind === "message" && entry.role === "assistant")).toBe(true);
    await loader.unload("fixture-custom-agent");
    parent.close();
    child.close();
  });

  test("subagent_history_is_isolated", async () => {
    const loader = await loadCustomLoader();
    const parent = await SessionDriver.create(803);
    const parentContext: CustomAgentContext = { session: parent.session, model: parent.provider, signal: new AbortController().signal };
    parent.session.appendMessage({ role: "user", content: [{ type: "text", text: "parent" }] });
    const child = await SessionDriver.create(804);
    child.enqueue(textTurn("child"));
    const contribution = loader.registries.agents.get("custom");
    if (!contribution?.create) throw new Error("custom agent was not registered");
    const before = parent.session.snapshot().entries.length;
    const result = await runIndependentSubagent(parentContext, (context) => contribution.create!(context) as { run(): Promise<never> }, child.provider);
    expect(result.childSession.snapshot().sessionId).not.toBe(parent.session.snapshot().sessionId);
    expect(parent.session.snapshot().entries.length).toBe(before);
    expect(child.provider.requests).toHaveLength(1);
    expect(parent.provider.requests).toHaveLength(0);
    await loader.unload("fixture-custom-agent");
    parent.close();
    child.close();
  });
});

async function loadCustomLoader(): Promise<PackageLoader> {
  const candidates = await discoverPackages({ user: [{ root: fixtureRoot, manifest: { name: "fixture-custom-agent", version: "1.0.0", keywords: ["iyon-package"], iyon: { extensions: "./custom-agent.ts" } } }] });
  const loader = new PackageLoader();
  await loader.loadOrThrow(candidates[0]!);
  return loader;
}

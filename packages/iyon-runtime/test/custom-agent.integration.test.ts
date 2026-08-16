import { describe, expect, test } from "bun:test";
import { installIyonVirtualModules } from "../src/virtual-modules.ts";
import { discoverPackages, PackageLoader } from "@iyon/plugins";
import { SessionDriver } from "../../../plugins/agents/iyon/test/support/session-driver.ts";
import { textTurn } from "../../../plugins/agents/iyon/test/support/fixtures.ts";
import type { CustomAgentContext } from "./fixtures/custom-agent.ts";

installIyonVirtualModules();

const fixtureRoot = new URL("./fixtures/", import.meta.url).pathname;

describe("T9 custom agent proof", () => {
  test("custom_agent_uses_only_public_core_and_differs_from_iyon", async () => {
    const { loader, context, driver } = await loadCustomAgent(701);
    driver.session.appendMessage({ role: "user", content: [{ type: "text", text: "first" }] });
    driver.session.appendMessage({ role: "user", content: [{ type: "text", text: "last" }] });
    driver.enqueue(textTurn("custom"), textTurn("unused"));
    const agent = customAgent(loader, context);
    await agent.run();
    expect(driver.provider.requests).toHaveLength(1);
    expect(driver.provider.requests[0]?.systemPrompt).toBe("custom-agent-only");
    expect(driver.provider.requests[0]?.messages).toHaveLength(1);
    expect(driver.provider.requests[0]?.messages[0]?.content[0]).toEqual({ type: "text", text: "last" });
    await loader.unload("fixture-custom-agent");
    driver.close();
  });

  test("custom_agent_can_select_a_subset_of_tools", async () => {
    const { loader, context, driver } = await loadCustomAgent(702);
    const tools = { list: () => [{ value: { name: "first", description: "first", inputSchema: {} } }, { value: { name: "second", description: "second", inputSchema: {} } }] };
    driver.enqueue(textTurn("custom"));
    await customAgent(loader, { ...context, tools }).run();
    expect(driver.provider.requests[0]?.tools.map((tool) => tool.name)).toEqual(["first"]);
    await loader.unload("fixture-custom-agent");
    driver.close();
  });

  test("custom_agent_can_transform_context", async () => {
    const { loader, context, driver } = await loadCustomAgent(703);
    driver.session.appendMessage({ role: "user", content: [{ type: "text", text: "old" }] });
    driver.session.appendMessage({ role: "assistant", content: [{ type: "text", text: "answer" }] });
    driver.session.appendMessage({ role: "user", content: [{ type: "text", text: "new" }] });
    driver.enqueue(textTurn("custom"));
    await customAgent(loader, context).run();
    expect(driver.provider.requests[0]?.messages.map((message) => message.content[0])).toEqual([{ type: "text", text: "new" }]);
    await loader.unload("fixture-custom-agent");
    driver.close();
  });

  test("custom_agent_has_a_custom_stop_condition", async () => {
    const { loader, context, driver } = await loadCustomAgent(704);
    driver.enqueue(textTurn("one"), textTurn("two"));
    await customAgent(loader, context).run();
    expect(driver.provider.requests).toHaveLength(1);
    await loader.unload("fixture-custom-agent");
    driver.close();
  });
});

async function loadCustomAgent(id: number) {
  const candidates = await discoverPackages({ user: [{ root: fixtureRoot, manifest: { name: "fixture-custom-agent", version: "1.0.0", keywords: ["iyon-package"], iyon: { extensions: "./custom-agent.ts" } } }] });
  const loader = new PackageLoader();
  await loader.loadOrThrow(candidates[0]!);
  const driver = await SessionDriver.create(id);
  const context: CustomAgentContext = { session: driver.session, model: driver.provider, signal: new AbortController().signal };
  return { loader, context, driver };
}

function customAgent(loader: PackageLoader, context: CustomAgentContext): { run(): Promise<unknown> } {
  const contribution = loader.registries.agents.get("custom");
  if (!contribution?.create) throw new Error("custom agent was not registered");
  return contribution.create(context) as { run(): Promise<unknown> };
}

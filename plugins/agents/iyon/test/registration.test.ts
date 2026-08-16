import { describe, expect, test } from "bun:test";
import { installIyonVirtualModules } from "@iyon/runtime";
import { discoverPackageRoot } from "@iyon/plugins";
import { PackageLoader } from "@iyon/plugins";
import { SessionDriver } from "./support/session-driver.ts";
import { textTurn } from "./support/fixtures.ts";
import type { AgentContext } from "../src/agent.ts";

installIyonVirtualModules();

const packageRoot = new URL("../", import.meta.url).pathname;

describe("bundled Iyon agent registration", () => {
  test("registers through the normal extension loader", async () => {
    const loader = new PackageLoader();
    const candidate = await discoverPackageRoot(packageRoot, "bundled");
    await loader.loadOrThrow(candidate);
    const contribution = loader.registries.agents.get("iyon");
    expect(contribution?.id).toBe("iyon");
    expect(contribution?.create).toBeFunction();
    await loader.unload("@iyon/agent-iyon");
    expect(loader.registries.agents.get("iyon")).toBeUndefined();
  });

  test("created agent runs against a public scripted session", async () => {
    const loader = new PackageLoader();
    const candidate = await discoverPackageRoot(packageRoot, "bundled");
    await loader.loadOrThrow(candidate);
    const contribution = loader.registries.agents.get("iyon");
    if (!contribution?.create) throw new Error("bundled Iyon contribution has no factory");
    const driver = await SessionDriver.create(601);
    driver.session.appendMessage({ role: "user", content: [{ type: "text", text: "hello" }] });
    driver.enqueue(textTurn("world"));
    const context: AgentContext = { session: driver.session, model: driver.provider, signal: new AbortController().signal };
    await (contribution.create(context) as { run(): Promise<void> }).run();
    expect(driver.provider.requests).toHaveLength(1);
    driver.close();
    await loader.unload("@iyon/agent-iyon");
  });

  test("duplicate registration rolls back without a privileged branch", async () => {
    const loader = new PackageLoader();
    loader.registries.agents.register({ id: "iyon", create: () => ({}) });
    const candidate = await discoverPackageRoot(packageRoot, "bundled");
    const failures = await loader.load(candidate);
    expect(failures[0]?.ok).toBe(false);
    expect(loader.registries.agents.get("iyon")?.create).toBeFunction();
  });
});

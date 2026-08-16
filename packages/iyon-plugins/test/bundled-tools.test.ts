import { describe, expect, test } from "bun:test";
import { installIyonVirtualModules } from "../../iyon-runtime/src/virtual-modules.ts";
import { discoverPackageRoot } from "../src/discovery.ts";
import { createRegistries } from "../src/loader.ts";
import { registerBundledTools } from "../src/bundled-tools.ts";

installIyonVirtualModules();

describe("bundled tool registration", () => {
  test("loads all seven tools through the ordinary package loader", async () => {
    const loader = await registerBundledTools();
    expect(loader.registries.tools.list().map((entry) => entry.id)).toEqual(["bash", "edit", "find", "grep", "ls", "read", "write"]);
    expect(loader.registries.tools.list().every((entry) => "execute" in entry.value && "renderCall" in entry.value && "renderResult" in entry.value)).toBe(true);
  });

  test("replaces a complete read contribution atomically", async () => {
    const loader = await registerBundledTools({ registries: createRegistries() });
    const candidate = await discoverPackageRoot(new URL("./fixtures/replace-read/", import.meta.url).pathname, "project");
    await loader.loadOrThrow(candidate);
    const read = loader.registries.tools.lookup("read")?.value as unknown as { execute: () => Promise<{ content: [{ text: string }] }>; renderCall: () => { kind: string }; renderResult: () => { kind: string } };
    expect((await read.execute()).content[0]?.text).toBe("replacement execution");
    expect(read.renderCall().kind).toBe("view");
    expect(read.renderResult().kind).toBe("view");
  });

  test("rolls back a failed replacement without losing the original", async () => {
    const loader = await registerBundledTools({ registries: createRegistries() });
    const candidate = await discoverPackageRoot(new URL("./fixtures/replace-read/", import.meta.url).pathname, "project");
    const failing = { ...candidate, manifest: { ...candidate.manifest, extensions: [{ ...candidate.manifest.extensions[0]!, id: "failing", entrypoint: new URL("./fixtures/replace-read/src/failing.ts", import.meta.url).pathname, relativeEntrypoint: "./src/failing.ts" }] } };
    const result = await loader.load(failing);
    expect(result[0]?.ok).toBe(false);
    expect((loader.registries.tools.lookup("read")?.value as unknown as { description: string }).description).toBe("Read a UTF-8 text file from the workspace.");
  });
});

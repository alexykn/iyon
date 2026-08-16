import { describe, expect, test } from "bun:test";
import { createRegistries } from "../src/loader.ts";
import { EventHub } from "../src/events.ts";
import { ExtensionContext } from "../src/extension-api.ts";

describe("ExtensionAPI", () => {
  test("exposes all contribution surfaces with loader-owned metadata", () => {
    const registries = createRegistries();
    const resources: { dispose(): void }[] = [];
    const api = new ExtensionContext({ packageId: "package", extensionId: "extension", scope: "user", generation: 1, source: { type: "local", path: "/package", descriptor: "local:/package" } }, registries, new EventHub(), (name, value, options) => {
      const contribution = (registries[name] as any).registerOwned(value, { packageId: "package", extensionId: "extension", scope: "user", source: { type: "local", path: "/package", descriptor: "local:/package" } }, options);
      resources.push(contribution.dispose);
      return contribution.dispose;
    });
    api.tools.register({ id: "owned" });
    expect(api.tools.lookup("owned")?.source.packageId).toBe("package");
    expect(api.scene).toBeDefined();
    expect(api.on).toBeFunction();
    for (const resource of resources) resource.dispose();
    expect(registries.tools.lookup("owned")).toBeUndefined();
  });
});

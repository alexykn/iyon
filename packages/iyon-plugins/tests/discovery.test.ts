import { describe, expect, test } from "bun:test";
import { discoverPackages } from "../src/discovery.ts";
import { normalizePackageSource } from "../src/package-source.ts";

const manifest = (name: string) => ({ name, version: "1.0.0", keywords: ["iyon-package"], iyon: { extensions: "./index.ts" } });

describe("package discovery", () => {
  test("orders bundled, user, and project candidates deterministically", async () => {
    const options = {
      bundled: [{ root: "/bundled/z", manifest: manifest("z") }, { root: "/bundled/a", manifest: manifest("a") }],
      user: [{ root: "/user/b", manifest: manifest("b") }],
      project: [{ root: "/project/c", manifest: manifest("c") }],
    } as const;
    const first = await discoverPackages(options);
    const second = await discoverPackages(options);
    expect(first.map((candidate) => `${candidate.scope}:${candidate.manifest.packageId}`)).toEqual(["bundled:a", "bundled:z", "user:b", "project:c"]);
    expect(second.map((candidate) => candidate.manifest.packageId)).toEqual(first.map((candidate) => candidate.manifest.packageId));
  });

  test("normalizes all supported source descriptors", () => {
    expect(normalizePackageSource("npm:example@1.2.3").descriptor).toBe("npm:example@1.2.3");
    expect(normalizePackageSource({ type: "git", url: "https://example.test/plugin.git", ref: "main" }).descriptor).toBe("git:https://example.test/plugin.git#main");
    const local = normalizePackageSource({ type: "local", path: "packages/plugin" }, "/repo");
    expect(local.type === "local" ? local.path : undefined).toBe("/repo/packages/plugin");
  });
});

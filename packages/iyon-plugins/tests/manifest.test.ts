import { describe, expect, test } from "bun:test";
import { normalizeManifest } from "../src/manifest.ts";
import { CompatibilityError, ManifestError } from "../src/errors.ts";
import { validateCompatibility } from "../src/compatibility.ts";

const root = "/tmp/iyon-fixture";

describe("extension manifests", () => {
  test("normalizes shorthand and expanded entrypoints without using catalog as authority", () => {
    const shorthand = normalizeManifest({ name: "fixture", version: "1.0.0", keywords: ["iyon-package"], iyon: { extensions: "./src/index.ts", catalog: { extensions: ["wrong"] } } }, root);
    const expanded = normalizeManifest({ name: "fixture", version: "1.0.0", keywords: ["iyon-package"], iyon: { extensions: { fixture: "./src/index.ts" }, catalog: { extensions: ["wrong"] } } }, root);
    expect(shorthand.extensions[0]?.entrypoint).toBe(expanded.extensions[0]?.entrypoint);
    expect(shorthand.extensions[0]?.id).toBe("fixture");
    expect(shorthand.catalog).toEqual({ extensions: ["wrong"] });
  });

  test("rejects packages without the discoverability marker", () => {
    expect(() => normalizeManifest({ name: "bad", version: "1.0.0", iyon: { extensions: "./index.ts" } }, root)).toThrow(ManifestError);
  });

  test("reports incompatible runtime requirements before loading", () => {
    const manifest = normalizeManifest({ name: "future", version: "1.0.0", keywords: ["iyon-package"], iyon: { extensions: "./index.ts", compatibility: { runtime: ">=2.0.0" } } }, root);
    expect(() => validateCompatibility(manifest, { version: "1.0.0" })).toThrow(CompatibilityError);
  });
});

/**
 * Standalone application ownership gates for repository separation S5.
 *
 * The application consumes the generic TUI through exact external revisions.
 * This checker rejects local TUI copies, deep imports, and TUI symbols in the
 * application native addon.
 */

import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";

const ROOT = resolve(import.meta.dir, "../..");
const TUI_REVISION = "e322f10dff490c1423d988982c0782c22774f85d";
let failed = false;

function pass(name: string, detail?: string): void {
  console.log(`PASS ${name}${detail ? ` — ${detail}` : ""}`);
}

function fail(name: string, detail: string): void {
  failed = true;
  console.log(`FAIL ${name} — ${detail}`);
}

function walk(dir: string, out: string[] = []): string[] {
  if (!existsSync(dir)) return out;
  for (const entry of Array.from(readdirSync(dir)).sort()) {
    if (entry === "node_modules" || entry.startsWith(".")) continue;
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) walk(path, out);
    else out.push(path);
  }
  return out;
}

function specifiersOf(source: string): string[] {
  return [
    ...[...source.matchAll(/(?:^|\s)from\s+["']([^"']+)["']/g)].map((match) => match[1]!),
    ...[...source.matchAll(/import\(\s*["']([^"']+)["']\s*\)/g)].map((match) => match[1]!),
    ...[...source.matchAll(/(?:^|\s)import\s+["']([^"']+)["']/g)].map((match) => match[1]!),
  ];
}

function parseJson(path: string): Record<string, any> {
  return JSON.parse(readFileSync(path, "utf8")) as Record<string, any>;
}

function noLocalTuiPathsGate(): void {
  const forbidden = [
    "crates/iyon-tui",
    "crates/iyon-native",
    "packages/iyon-runtime/src/tui",
    "packages/iyon-runtime/bench",
    "tools/tui-abi",
    "tools/tui-abi-gen",
    "PERF-11-generated-abi-reference.md",
  ];
  const present = forbidden.filter((path) => existsSync(join(ROOT, path)));
  if (present.length > 0) fail("no-local-tui-compatibility", present.join(", "));
  else pass("no-local-tui-compatibility", "temporary local TUI paths are absent");
}

function rustConsumerGate(): void {
  const manifest = readFileSync(join(ROOT, "Cargo.toml"), "utf8");
  const expected = `iyon-tui = { git = "https://github.com/alexykn/iyon-tui.git", rev = "${TUI_REVISION}" }`;
  const violations: string[] = [];
  if (!manifest.includes(expected)) violations.push("workspace iyon-tui dependency is not pinned to the S5 revision");
  if (/iyon-tui\s*=\s*\{\s*path\s*=/.test(manifest)) violations.push("workspace iyon-tui dependency uses a local path");

  const nativeManifest = join(ROOT, "crates/iyon-core-native/Cargo.toml");
  if (!existsSync(nativeManifest)) violations.push("crates/iyon-core-native/Cargo.toml is missing");
  else if (/iyon-tui|native-host|view_abi/i.test(readFileSync(nativeManifest, "utf8"))) {
    violations.push("core-native manifest mentions TUI/native View ABI dependencies");
  }

  const nativeSources = walk(join(ROOT, "crates/iyon-core-native")).filter((path) => path.endsWith(".rs"));
  const nativeHits = nativeSources.filter((path) => /iyon_tui|NativeTui|tuiView|view_abi|generated_view_abi/.test(readFileSync(path, "utf8")));
  if (nativeHits.length > 0) violations.push(`core-native TUI symbols: ${nativeHits.map((path) => relative(ROOT, path)).join(", ")}`);

  if (violations.length > 0) fail("external-rust-tui-consumer", violations.join("; "));
  else pass("external-rust-tui-consumer", `iyon-tui is pinned to ${TUI_REVISION.slice(0, 12)}`);
}

function nativeContractGate(): void {
  const nativePath = join(ROOT, "packages/iyon-runtime/src/native.ts");
  const source = readFileSync(nativePath, "utf8");
  const violations: string[] = [];
  if (!source.includes("NativeCoreAddon")) violations.push("NativeCoreAddon contract is missing");
  if (/\bNativeAddon\b|NativeTui|NativeView|tuiView|NativeHistory|NativeTextStream|NativeScrollPane/.test(source)) {
    violations.push("application native contract still exposes TUI or shared-addon names");
  }
  if (!source.includes("iyon-core-native.node")) violations.push("core-native addon path is not loaded");
  if (source.includes("iyon-native.node")) violations.push("obsolete iyon-native.node path remains");
  if (violations.length > 0) fail("core-native-contract-purity", violations.join("; "));
  else pass("core-native-contract-purity", "NativeCoreAddon contains application/kernel exports only");
}

function tsConsumerGate(): void {
  const roots = [
    "plugins",
    "packages/iyon-cli/src",
    "packages/iyon-cli/test",
    "packages/iyon-plugins/src",
    "packages/iyon-plugins/tests",
    "packages/iyon-plugins/test",
    "packages/iyon-runtime/src",
    "packages/iyon-runtime/test",
    "packages/iyon-runtime/tests",
    "packages/iyon-sdk/src",
    "packages/iyon-sdk/tests",
  ];
  const violations: string[] = [];
  let filesChecked = 0;
  for (const root of roots) {
    for (const file of walk(join(ROOT, root)).filter((path) => path.endsWith(".ts") || path.endsWith(".d.ts"))) {
      filesChecked += 1;
      for (const spec of specifiersOf(readFileSync(file, "utf8"))) {
        if (spec === "@iyon/tui" || spec === "iyon:tui") continue;
        if (spec === "@iyon/tui/testing") {
          const relativeFile = relative(ROOT, file);
          if (/(^|\/)(?:test|tests)(?:\/|$)/u.test(relativeFile)) continue;
        }
        if (spec.startsWith("@iyon/tui/") || spec === "@iyon/runtime/tui" || spec.startsWith("@iyon/runtime/tui/")) {
          violations.push(`${relative(ROOT, file)} -> "${spec}"`);
          continue;
        }
        if (spec.startsWith(".") || spec.startsWith("/")) {
          const resolved = resolve(dirname(file), spec);
          if (resolved.includes(`${join(ROOT, "packages/iyon-runtime/src/tui")}/`)) {
            violations.push(`${relative(ROOT, file)} -> "${spec}" enters a local TUI path`);
          }
        }
      }
    }
  }
  if (violations.length > 0) fail("application-ts-public-tui-entrypoints", violations.join("; "));
  else pass("application-ts-public-tui-entrypoints", `${filesChecked} TypeScript files use public external TUI entrypoints only`);
}

function packagePinGate(): void {
  const manifests = [
    "package.json",
    "packages/iyon-runtime/package.json",
    "packages/iyon-plugins/package.json",
    "plugins/app/iyon/package.json",
    "plugins/tools/edit/package.json",
  ];
  const expected = `github:alexykn/iyon-tui#${TUI_REVISION}`;
  const violations: string[] = [];
  for (const path of manifests) {
    const manifest = parseJson(join(ROOT, path));
    if (manifest.dependencies?.["@iyon/tui"] !== expected) {
      violations.push(`${path} does not pin @iyon/tui to ${TUI_REVISION.slice(0, 12)}`);
    }
  }
  if (violations.length > 0) fail("external-ts-tui-consumer", violations.join("; "));
  else pass("external-ts-tui-consumer", `${manifests.length} manifests pin @iyon/tui to ${TUI_REVISION.slice(0, 12)}`);
}

noLocalTuiPathsGate();
rustConsumerGate();
nativeContractGate();
tsConsumerGate();
packagePinGate();

if (failed) {
  console.log("\nAPPLICATION OWNERSHIP CHECKS FAILED");
  process.exit(1);
}
console.log("\nALL APPLICATION OWNERSHIP CHECKS PASSED");

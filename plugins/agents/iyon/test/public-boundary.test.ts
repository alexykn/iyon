import { describe, expect, test } from "bun:test";
import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";

const sourceRoot = new URL("../src/", import.meta.url).pathname;

describe("bundled agent public boundary", () => {
  test("bundled_iyon_has_no_privileged_imports", async () => {
    const files = await sourceFiles(sourceRoot);
    const forbidden = /iyon-native|crates\/|from ["'](?:\.\.\/)+.*(?:native|crates)|NAPI|run_agent_loop|run_model_turn/;
    for (const file of files) expect(forbidden.test(await readFile(file, "utf8"))).toBe(false);
  });

  test("agent_sources_have_no_parallel_tool_path", async () => {
    const files = await sourceFiles(sourceRoot);
    for (const file of files) expect((await readFile(file, "utf8")).includes("Promise.all")).toBe(false);
  });
});

async function sourceFiles(root: string): Promise<string[]> {
  const entries = await readdir(root, { withFileTypes: true });
  const files: string[] = [];
  for (const entry of entries) {
    const path = join(root, entry.name);
    if (entry.isDirectory()) files.push(...await sourceFiles(path));
    else if (entry.name.endsWith(".ts")) files.push(path);
  }
  return files;
}

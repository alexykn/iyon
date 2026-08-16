import { expect, test } from "bun:test";
import { readFile } from "node:fs/promises";

test("the shared loader has no built-in execution shortcut", async () => {
  const source = await readFile(new URL("../src/loader.ts", import.meta.url), "utf8");
  expect(source).not.toContain("is_builtin");
  expect(source).not.toContain("isBuiltin");
});

import { describe, expect, test } from "bun:test";
import { mkdtemp } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

const repositoryDirectory = join(import.meta.dir, "../../..");
const standalone = join(repositoryDirectory, "iyon-smoke");

describe("T1 standalone distribution", () => {
  test("addon_is_embedded_in_standalone_executable", async () => {
    const emptyWorkingDirectory = await mkdtemp(join(tmpdir(), "iyon-t1-"));
    const result = Bun.spawnSync({
      cmd: [standalone],
      cwd: emptyWorkingDirectory,
      stdout: "pipe",
      stderr: "pipe",
    });
    const stdout = new TextDecoder().decode(result.stdout).trim();
    const stderr = new TextDecoder().decode(result.stderr).trim();

    expect(result.exitCode).toBe(0);
    expect(stderr).toBe("");
    expect(JSON.parse(stdout)).toEqual({
      ok: true,
      native: "iyon-native/t1",
      tui: "iyon:tui/t1",
      concurrent: 100,
      event: "fifo-and-close",
    });
  });
});

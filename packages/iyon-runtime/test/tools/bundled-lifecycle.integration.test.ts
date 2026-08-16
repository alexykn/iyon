import { describe, expect, test } from "bun:test";
import { mkdtemp, mkdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { KernelSession } from "../../src/modules/core.ts";
import { executeTool } from "../../src/tools/execution.ts";
import { renderGenericCall, renderGenericResult } from "../../src/tools/generic.ts";
import { registerBundledTools } from "@iyon/plugins";
import type { JsonValue } from "@iyon/sdk";
import type { AnyTool } from "../../src/tools/contract.ts";

const toolArgs: Record<string, unknown> = {
  read: { path: "README.md" },
  write: { path: "out.txt", content: "written\n" },
  edit: { path: "README.md", edits: [{ oldText: "hello", newText: "HELLO" }] },
  ls: {},
  find: { pattern: "*.md" },
  grep: { pattern: "hello", literal: true },
  bash: { command: "printf bun" },
};

describe("bundled Bun product lifecycle", () => {
  test("drives every bundled contribution through native lifecycle and rendering", async () => {
    const root = await mkdtemp(join(tmpdir(), "iyon-bundled-lifecycle-"));
    await mkdir(join(root, "src"));
    await writeFile(join(root, "README.md"), "hello\n");
    const loader = await registerBundledTools();
    for (const name of ["bash", "read", "write", "edit", "grep", "find", "ls"]) {
      const tool = loader.registries.tools.lookup(name)?.value as unknown as AnyTool;
      const session = new KernelSession({ id: 50 });
      const request = { sessionId: 50 as never, turnId: 1 as never, messageId: 2 as never, toolCallId: `${name}-call` as never, toolName: name, arguments: toolArgs[name] as JsonValue };
      const result = await executeTool(session, tool, request, { cwd: root, workspace: { root } });
      expect(result.execution.state()).toBe("finished");
      expect(result.execution.events().map((event) => event.state)).toContain("finished");
      expect(tool.renderCall({ ...request, id: request.toolCallId, name, state: "running" })).toMatchObject({ kind: "view" });
      expect(tool.renderResult(result.result)).toMatchObject({ kind: "view" });
      session.close();
    }
  });

  test("keeps unknown third-party tools on the generic path", async () => {
    const root = await mkdtemp(join(tmpdir(), "iyon-unknown-tool-"));
    const session = new KernelSession({ id: 51 });
    const result = await executeTool(session, undefined, { sessionId: 51 as never, turnId: 1 as never, messageId: 2 as never, toolCallId: "weather-call" as never, toolName: "weather", arguments: { city: "Berlin" } }, { cwd: root, workspace: { root } });
    expect(result.result.isError).toBe(true);
    expect(renderGenericCall({ id: "weather-call" as never, name: "weather", arguments: { city: "Berlin" }, state: "finished", showArgPreview: true })).toMatchObject({ kind: "view" });
    expect(renderGenericResult(result.result)).toMatchObject({ kind: "view" });
    session.close();
  });
});

import * as app from "../../../plugins/app/iyon/src/index.ts";
import * as agent from "../../../plugins/agents/iyon/src/index.ts";
import * as mock from "../../../plugins/providers/mock/src/index.ts";
import * as codex from "../../../plugins/providers/openai-codex/src/index.ts";
import * as openrouter from "../../../plugins/providers/openrouter/src/index.ts";
import * as bash from "../../../plugins/tools/bash/src/index.ts";
import * as edit from "../../../plugins/tools/edit/src/index.ts";
import * as find from "../../../plugins/tools/find/src/index.ts";
import * as grep from "../../../plugins/tools/grep/src/index.ts";
import * as ls from "../../../plugins/tools/ls/src/index.ts";
import * as read from "../../../plugins/tools/read/src/index.ts";
import * as write from "../../../plugins/tools/write/src/index.ts";

type ExtensionModule = { readonly activate?: (...args: readonly never[]) => unknown };
const sources: Readonly<Record<string, ExtensionModule>> = { app, agent, mock, codex, openrouter, bash, edit, find, grep, ls, read, write };
const paths = Object.fromEntries(Object.entries(sources).map(([name, module]) => [extensionRelativePath(name), module]));

export function installBundledExtensionModules(): void {
  const globalModules = globalThis as typeof globalThis & { __iyonBundledExtensions?: Readonly<Record<string, ExtensionModule>> };
  globalModules.__iyonBundledExtensions = paths;
  Bun.plugin({
    name: "iyon-embedded-extensions",
    setup(build) {
      build.onResolve({ filter: /.*/ }, ({ path }) => {
        // Match by suffix so that imports from the worktree path (
        // e.g. <cache>/tui-branches/app-.../plugins/app/iyon/src/index.ts)
        // resolve to the same bundled module as imports from the repo path.
        for (const [extPath, module] of Object.entries(paths)) {
          if (path.endsWith(extPath)) {
            return { path: extPath, namespace: "iyon-embedded-extension" };
          }
        }
        return undefined;
      });
      build.onLoad({ filter: /.*/, namespace: "iyon-embedded-extension" }, ({ path }) => ({ contents: `export const activate = globalThis.__iyonBundledExtensions[${JSON.stringify(path)}].activate;`, loader: "js" }));
    },
  });
}

function extensionRelativePath(name: string): string {
  const directory = name === "app" ? "app/iyon" : name === "agent" ? "agents/iyon" : name === "mock" ? "providers/mock" : name === "codex" ? "providers/openai-codex" : name === "openrouter" ? "providers/openrouter" : `tools/${name}`;
  return `plugins/${directory}/src/index.ts`;
}

import {
  CancellationProbe,
  EventQueueProbe,
  NativeCounter,
  asyncSleep,
  echoBuffer,
  echoJson,
  echoString,
  nativeCounterStats,
  nativeVersion,
  resetNativeCounterStats,
} from "./native.ts";
import {
  apiSmoke,
  cancellationOperation,
  coreSmoke,
  runWithAbortSignal,
  tuiSmoke,
} from "./smoke.ts";

const virtualModules = {
  "iyon:api": `
    export { apiSmoke } from "@iyon/runtime/smoke";
    export { nativeVersion, echoJson, echoString, echoBuffer } from "@iyon/runtime/native";
  `,
  "iyon:core": `
    export { coreSmoke, runWithAbortSignal, cancellationOperation } from "@iyon/runtime/smoke";
    export {
      asyncSleep,
      CancellationProbe,
      NativeCounter,
      EventQueueProbe,
      nativeCounterStats,
      resetNativeCounterStats,
    } from "@iyon/runtime/native";
  `,
  "iyon:tui": `
    export { tuiSmoke } from "@iyon/runtime/smoke";
  `,
} as const;

type IyonVirtualModule = keyof typeof virtualModules;

export const iyonVirtualModulePlugin: Bun.BunPlugin = {
  name: "iyon-t1-virtual-modules",
  setup(build) {
    build.onResolve(
      { filter: /^(iyon:)?(api|core|tui)$/ },
      ({ path }) => ({
        path: path.startsWith("iyon:") ? path : `iyon:${path}`,
        namespace: "iyon-t1-virtual",
      }),
    );
    build.onLoad(
      { filter: /^(iyon:)?(api|core|tui)$/, namespace: "iyon-t1-virtual" },
      ({ path }) => {
        const moduleName = path.startsWith("iyon:") ? path : `iyon:${path}`;
        const source = virtualModules[moduleName as IyonVirtualModule];
        if (source === undefined) {
          throw new Error(`unknown Iyon virtual module: ${path}`);
        }
        return { contents: source, loader: "ts" };
      },
    );

  },
};

function registerRuntimeModules(build: Bun.PluginBuilder): void {
  // Bun 1.3 applies `module` to runtime dynamic imports while
  // onResolve/onLoad are the bundler path for the same canonical names.
  build.module("iyon:api", () => ({
    exports: { apiSmoke, nativeVersion, echoJson, echoString, echoBuffer },
    loader: "object",
  }));
  build.module("iyon:core", () => ({
    exports: {
      coreSmoke,
      runWithAbortSignal,
      cancellationOperation,
      asyncSleep,
      CancellationProbe,
      NativeCounter,
      EventQueueProbe,
      nativeCounterStats,
      resetNativeCounterStats,
    },
    loader: "object",
  }));
  build.module("iyon:tui", () => ({
    exports: { tuiSmoke },
    loader: "object",
  }));
}

let installed = false;

/** Install the three T1 canonical modules once, before application imports. */
export function installIyonVirtualModules(): void {
  if (installed) {
    return;
  }
  Bun.plugin({
    name: iyonVirtualModulePlugin.name,
    setup(build) {
      iyonVirtualModulePlugin.setup(build);
      registerRuntimeModules(build);
    },
  });
  installed = true;
}

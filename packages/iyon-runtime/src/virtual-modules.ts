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

let installed = false;

/** Install the three T1 canonical modules once, before application imports. */
export function installIyonVirtualModules(): void {
  if (installed) {
    return;
  }

  Bun.plugin({
    name: "iyon-t1-virtual-modules",
    setup(build) {
      build.onResolve(
        { filter: /^iyon:(api|core|tui)$/ },
        ({ path }) => ({
          path,
          namespace: "iyon-t1-virtual",
        }),
      );
      build.onLoad(
        { filter: /^iyon:(api|core|tui)$/, namespace: "iyon-t1-virtual" },
        ({ path }) => {
          const source = virtualModules[path as IyonVirtualModule];
          if (source === undefined) {
            throw new Error(`unknown Iyon virtual module: ${path}`);
          }
          return { contents: source, loader: "ts" };
        },
      );

      // Bun 1.3 applies `module` to runtime dynamic imports while
      // onResolve/onLoad remain the bundler path for the same canonical names.
      // Both paths expose the identical smoke-only surface.
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
    },
  });
  installed = true;
}
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

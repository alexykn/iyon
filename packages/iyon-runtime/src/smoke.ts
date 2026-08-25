import { native } from "./native.ts";
import { tuiSmoke } from "@iyon/tui";
import { runWithAbortSignal } from "./modules/abort.ts";

export { runWithAbortSignal } from "./modules/abort.ts";

export const apiSmoke = "iyon:api/t1" as const;
export const coreSmoke = "iyon:core/t1" as const;
export { tuiSmoke };

export type CancellableOperation<T> = {
  run(): Promise<T>;
  cancel(): void;
};

/**
 * A dropped or unreferenced JS Promise does not cancel Rust work. This helper
 * bridges AbortSignal to the native handle's explicit cancel method and always
 * removes its listener after the native Promise settles.
 */
export function cancellationOperation(ms: number): CancellableOperation<string> {
  const probe = new native.CancellationProbe();
  return {
    run: () => probe.run(ms),
    cancel: () => probe.cancel(),
  };
}

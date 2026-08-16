import { native } from "./native.ts";

export const apiSmoke = "iyon:api/t1" as const;
export const coreSmoke = "iyon:core/t1" as const;
export const tuiSmoke = "iyon:tui/t1" as const;

export type CancellableOperation<T> = {
  run(): Promise<T>;
  cancel(): void;
};

/**
 * A dropped or unreferenced JS Promise does not cancel Rust work. This helper
 * bridges AbortSignal to the native handle's explicit cancel method and always
 * removes its listener after the native Promise settles.
 */
export async function runWithAbortSignal<T>(
  signal: AbortSignal,
  operation: CancellableOperation<T>,
): Promise<T> {
  const cancel = (): void => operation.cancel();
  if (signal.aborted) {
    cancel();
  } else {
    signal.addEventListener("abort", cancel, { once: true });
  }

  try {
    return await operation.run();
  } finally {
    signal.removeEventListener("abort", cancel);
  }
}

export function cancellationOperation(ms: number): CancellableOperation<string> {
  const probe = new native.CancellationProbe();
  return {
    run: () => probe.run(ms),
    cancel: () => probe.cancel(),
  };
}

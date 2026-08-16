import { IyonNativeError, asIyonError } from "./errors.ts";

export interface CancellableOperation<T> {
  run(): Promise<T>;
  cancel(): void | Promise<void>;
}

export function abortError(): IyonNativeError {
  return new IyonNativeError("ION_CANCELLED", "operation cancelled");
}

export async function runWithAbortSignal<T>(
  signal: AbortSignal | undefined,
  operation: CancellableOperation<T>,
): Promise<T> {
  if (signal?.aborted) {
    throw abortError();
  }
  const cancel = (): void => {
    void operation.cancel();
  };
  signal?.addEventListener("abort", cancel, { once: true });
  try {
    return await operation.run();
  } catch (error) {
    throw asIyonError(error);
  } finally {
    signal?.removeEventListener("abort", cancel);
  }
}

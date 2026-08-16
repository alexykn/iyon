export type NativeErrorCode =
  | "ION_INVALID_INPUT"
  | "ION_INTERNAL"
  | "ION_CANCELLED"
  | "ION_CLOSED"
  | "ION_UNKNOWN";

export class IyonNativeError extends Error {
  readonly code: NativeErrorCode;

  constructor(code: NativeErrorCode, message: string, options?: { cause?: unknown }) {
    super(message, options);
    this.name = "IyonNativeError";
    this.code = code;
  }
}

export function asIyonError(error: unknown): IyonNativeError {
  if (error instanceof IyonNativeError) {
    return error;
  }
  const message = error instanceof Error ? error.message : String(error);
  const code = (message.match(/\b(ION_[A-Z_]+)\b/)?.[1] ?? "ION_UNKNOWN") as NativeErrorCode;
  return new IyonNativeError(code, message, { cause: error });
}

export function isCancelledError(error: unknown): boolean {
  return asIyonError(error).code === "ION_CANCELLED";
}

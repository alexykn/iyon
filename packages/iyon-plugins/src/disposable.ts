import { PluginError } from "./errors.ts";

export interface Disposable {
  dispose(): void | Promise<void>;
}

export type MaybeDisposable = Disposable | (() => void | Promise<void>);

export class DisposableStack implements Disposable {
  private readonly disposables: MaybeDisposable[] = [];
  private disposed = false;

  use<T extends MaybeDisposable>(disposable: T): T {
    if (this.disposed) throw new PluginError("activation", "cannot register a resource after its activation scope was disposed");
    this.disposables.push(disposable);
    return disposable;
  }

  get size(): number { return this.disposables.length; }

  async dispose(): Promise<void> {
    if (this.disposed) return;
    this.disposed = true;
    const errors: unknown[] = [];
    for (const disposable of [...this.disposables].reverse()) {
      try {
        await (typeof disposable === "function" ? disposable() : disposable.dispose());
      } catch (error) {
        errors.push(error);
      }
    }
    this.disposables.length = 0;
    if (errors.length === 1) throw errors[0];
    if (errors.length > 1) throw new AggregateError(errors, "multiple extension resources failed to dispose");
  }
}

export function asDisposable(value: MaybeDisposable): Disposable {
  return typeof value === "function" ? { dispose: value } : value;
}

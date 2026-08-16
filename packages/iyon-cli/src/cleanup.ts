export type CleanupTask = () => Promise<void> | void;

export class CleanupStack {
  private readonly tasks: CleanupTask[] = [];
  private closed = false;

  use(task: CleanupTask): void { if (this.closed) throw new Error("cleanup stack is closed"); this.tasks.push(task); }

  async close(): Promise<void> {
    if (this.closed) return;
    this.closed = true;
    const errors: unknown[] = [];
    for (const task of [...this.tasks].reverse()) { try { await task(); } catch (error) { errors.push(error); } }
    this.tasks.length = 0;
    if (errors.length > 0) throw new AggregateError(errors, "CLI cleanup failed");
  }
}

export async function withCleanup<T>(tasks: readonly CleanupTask[], operation: (cleanup: CleanupStack) => Promise<T>): Promise<T> {
  const cleanup = new CleanupStack(); tasks.forEach((task) => cleanup.use(task));
  try { return await operation(cleanup); } finally { await cleanup.close(); }
}

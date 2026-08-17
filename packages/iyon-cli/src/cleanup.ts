export type CleanupTask = () => Promise<void> | void;

interface CleanupEntry {
  readonly label: string;
  readonly task: CleanupTask;
}

export function isExpectedCleanupError(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  return /already (?:closed|restored)|(?:session|terminal) closed/u.test(message.toLowerCase());
}

export class CleanupStack {
  private readonly tasks: CleanupEntry[] = [];
  private closed = false;

  use(task: CleanupTask, label = "cleanup task"): void {
    if (this.closed) throw new Error("cleanup stack is closed");
    this.tasks.push({ label, task });
  }

  async close(): Promise<void> {
    if (this.closed) return;
    this.closed = true;
    const errors: Array<{ readonly label: string; readonly error: unknown }> = [];
    for (const { label, task } of [...this.tasks].reverse()) {
      try { await task(); } catch (error) {
        if (!isExpectedCleanupError(error)) errors.push({ label, error });
      }
    }
    this.tasks.length = 0;
    if (errors.length > 0) {
      const message = errors.map(({ label, error }) => `${label}: ${errorText(error)}`).join("; ");
      throw new AggregateError(errors.map(({ error }) => error), `cleanup failed: ${message}`);
    }
  }
}

export async function withCleanup<T>(tasks: readonly CleanupTask[], operation: (cleanup: CleanupStack) => Promise<T>): Promise<T> {
  const cleanup = new CleanupStack(); tasks.forEach((task) => cleanup.use(task));
  try { return await operation(cleanup); } finally { await cleanup.close(); }
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

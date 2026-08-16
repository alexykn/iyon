import type { ApprovalState } from "@iyon/sdk";

export interface ApprovalDecision {
  readonly approved: boolean;
  readonly reason?: string;
}

interface PendingApproval {
  readonly resolve: (decision: ApprovalDecision) => void;
  readonly reject: (error: Error) => void;
  readonly signal?: AbortSignal;
  readonly onAbort?: () => void;
}

export class ApprovalBroker {
  private readonly pending = new Map<number, PendingApproval>();

  request(state: ApprovalState, signal?: AbortSignal): Promise<ApprovalDecision> {
    if (signal?.aborted) return Promise.reject(abortReason(signal.reason));
    const abortSignal = signal;
    return new Promise((resolve, reject) => {
      const onAbort = abortSignal === undefined ? undefined : () => this.cancel(state.id, abortMessage(abortSignal!.reason));
      if (onAbort && abortSignal) abortSignal.addEventListener("abort", onAbort, { once: true });
      this.pending.set(state.id, { resolve, reject, signal: abortSignal, onAbort });
    });
  }

  approve(id: number): void {
    this.finish(id, { approved: true });
  }

  reject(id: number, reason = "approval rejected"): void {
    this.finish(id, { approved: false, reason });
  }

  cancel(id: number, reason = "approval cancelled"): void {
    const pending = this.take(id);
    pending?.reject(new Error(reason));
  }

  cancelAll(reason = "approvals cancelled"): void {
    for (const id of this.pending.keys()) this.cancel(id, reason);
  }

  private finish(id: number, decision: ApprovalDecision): void {
    this.take(id)?.resolve(decision);
  }

  private take(id: number): PendingApproval | undefined {
    const pending = this.pending.get(id);
    if (!pending) return undefined;
    this.pending.delete(id);
    if (pending.signal && pending.onAbort) pending.signal.removeEventListener("abort", pending.onAbort);
    return pending;
  }
}

function abortReason(reason: unknown): Error {
  return new Error(abortMessage(reason));
}

function abortMessage(reason: unknown): string {
  return reason instanceof Error ? reason.message : typeof reason === "string" ? reason : "approval cancelled";
}

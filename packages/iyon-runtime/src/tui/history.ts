import { materializeView } from "./materialize.ts";
import { HandleBase, nativeTui } from "./handles.ts";
import type { History as HistoryContract, HistoryLayout, TextStream } from "./types.ts";
import type { View } from "./values/view.ts";

export class History extends HandleBase<ReturnType<typeof nativeTui.history>, "history"> implements HistoryContract {
  constructor(nativeHandle = nativeTui.history()) { super("history", nativeHandle); }

  layout(): Promise<HistoryLayout> {
    return this.call(() => this.nativeHandle.layout() as HistoryLayout);
  }

  push(view: View): Promise<void> {
    return this.call(() => {
      const lowered = materializeView(view);
      if (lowered === undefined) throw new Error("native view materialization is unavailable");
      this.nativeHandle.push(lowered);
    });
  }

  pushStream(stream: TextStream): Promise<void> {
    return this.call(() => this.nativeHandle.pushStream((stream as unknown as { nativeObject(): object }).nativeObject()));
  }

  setLayout(layout: HistoryLayout): Promise<void> {
    return this.call(() => this.nativeHandle.setLayout(layout));
  }

  /** Internal bridge access; not exported from the public module. */
  nativeObject(): object { this.ensureOpen(); return this.nativeHandle; }
}

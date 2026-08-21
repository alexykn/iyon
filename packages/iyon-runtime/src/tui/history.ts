import { nodeForBridge, type View } from "./values/view.ts";
import { nativeViewAbiSession, releaseNativeViewRef, tryNativeMaterialize } from "./native_view_abi.ts";
import { HandleBase, nativeTui } from "./handles.ts";
import type { History as HistoryContract, HistoryLayout, TextStream } from "./types.ts";

export class History extends HandleBase<ReturnType<typeof nativeTui.history>, "history"> implements HistoryContract {
  constructor(nativeHandle = nativeTui.history()) { super("history", nativeHandle); }

  layout(): HistoryLayout {
    return this.call(() => this.nativeHandle.layout() as HistoryLayout);
  }

  push(view: View): number {
    return this.call(() => {
      const ref = tryNativeMaterialize(view);
      if (ref !== undefined && this.nativeHandle.pushRef !== undefined) {
        try {
          return this.nativeHandle.pushRef(ref);
        } finally {
          releaseNativeViewRef(nativeViewAbiSession(), ref);
        }
      }
      return this.nativeHandle.push(nodeForBridge(view));
    });
  }

  freeze(unit: number, view: View): void {
    this.call(() => {
      const ref = tryNativeMaterialize(view);
      if (ref !== undefined && this.nativeHandle.freezeRef !== undefined) {
        try {
          this.nativeHandle.freezeRef(unit, ref);
        } finally {
          releaseNativeViewRef(nativeViewAbiSession(), ref);
        }
        return;
      }
      this.nativeHandle.freeze(unit, nodeForBridge(view));
    });
  }

  discardLive(unit: number): void {
    this.call(() => this.nativeHandle.discardLive(unit));
  }

  pushStream(stream: TextStream): void {
    this.call(() => this.nativeHandle.pushStream((stream as unknown as { nativeObject(): object }).nativeObject()));
  }

  sealStream(stream: TextStream): void {
    this.call(() => this.nativeHandle.sealStream((stream as unknown as { nativeObject(): object }).nativeObject()));
  }

  setLayout(layout: HistoryLayout): void {
    this.call(() => this.nativeHandle.setLayout(layout));
  }

  /** Internal bridge access; not exported from the public module. */
  nativeObject(): object { this.ensureOpen(); return this.nativeHandle; }
}

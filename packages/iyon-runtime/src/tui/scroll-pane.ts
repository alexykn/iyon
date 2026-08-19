import { HandleBase } from "./handles.ts";
import { native } from "../native.ts";
import type { ComponentCapabilities, ScrollPane as ScrollPaneContract } from "./types.ts";
import { nodeForBridge, View } from "./values/view.ts";

type NativeScrollPaneHandle = {
  dispose(): void;
  componentId(): number | null;
  setContent(view: object): void;
  followEnd(): void;
};

export class NativeScrollPane extends HandleBase<NativeScrollPaneHandle, "component"> implements ScrollPaneContract {
  constructor(nativeHandle: NativeScrollPaneHandle | object) { super("component", nativeHandle as NativeScrollPaneHandle); }

  view(): View {
    this.ensureOpen();
    return this.nativeComponentId() === undefined ? View.spacer(0) : View.component(this);
  }

  capabilities(): ComponentCapabilities { return this.call(() => ({ focusable: true, keys: ["up", "down", "pageup", "pagedown", "home", "end"] })); }

  setContent(view: View): void {
    this.call(() => this.nativeHandle.setContent(nodeForBridge(view)));
  }

  followEnd(): void { this.call(() => this.nativeHandle.followEnd()); }

  nativeComponentId(): number | undefined {
    const id = this.nativeHandle.componentId();
    return id === null ? undefined : id;
  }
}

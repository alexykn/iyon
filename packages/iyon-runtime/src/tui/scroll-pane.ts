import { HandleBase } from "./handles.ts";
import { native } from "../native.ts";
import type { ComponentCapabilities, ScrollPane as ScrollPaneContract } from "./types.ts";
import { materializeView } from "./materialize.ts";
import { View } from "./values/view.ts";

type NativeScrollPaneHandle = {
  dispose(): void;
  componentId(): number | null;
  setContent(view: object): void;
  followEnd(): void;
};

export class NativeScrollPane extends HandleBase<NativeScrollPaneHandle, "component"> implements ScrollPaneContract {
  constructor(nativeHandle: NativeScrollPaneHandle | object) { super("component", nativeHandle as NativeScrollPaneHandle); }

  async view(): Promise<View> {
    this.ensureOpen();
    return this.nativeComponentId() === undefined ? View.spacer(0) : View.component(this);
  }

  capabilities(): Promise<ComponentCapabilities> { return this.call(() => ({ focusable: true, keys: ["up", "down", "pageup", "pagedown", "home", "end"] })); }

  setContent(view: View): Promise<void> {
    return this.call(() => {
      const lowered = materializeView(view);
      if (lowered === undefined) throw new Error("native view materialization is unavailable");
      this.nativeHandle.setContent(lowered as object);
    });
  }

  followEnd(): Promise<void> { return this.call(() => this.nativeHandle.followEnd()); }

  nativeComponentId(): number | undefined {
    const id = this.nativeHandle.componentId();
    return id === null ? undefined : id;
  }
}

import { HandleBase, requireNativeClass } from "./handles.ts";
import { native } from "../native.ts";
import type { Component as ComponentContract, ComponentCapabilities, ViewSlot as ViewSlotContract } from "./types.ts";
import { materializeView } from "./materialize.ts";
import { View } from "./values/view.ts";

type NativeViewSlotHandle = {
  dispose(): void;
  revision(): number;
  componentId(): number | null;
  setView(view: object): void;
};

export class ViewSlot extends HandleBase<NativeViewSlotHandle, "component"> implements ViewSlotContract {
  constructor(nativeHandle: NativeViewSlotHandle | object) { super("component", nativeHandle as NativeViewSlotHandle); }

  async view(): Promise<View> { this.ensureOpen(); return View.component(this); }
  capabilities(): Promise<ComponentCapabilities> { return this.call(() => ({})); }
  revision(): Promise<number> { return this.call(() => this.nativeHandle.revision()); }
  setView(view: View): Promise<void> {
    return this.call(() => {
      const lowered = materializeView(view);
      if (lowered === undefined) throw new Error("native view materialization is unavailable");
      this.nativeHandle.setView(lowered as object);
    });
  }
  nativeComponentId(): number | undefined {
    const id = this.nativeHandle.componentId();
    return id === null ? undefined : id;
  }
}

export class Component extends HandleBase<NativeViewSlotHandle, "component"> implements ComponentContract {
  constructor() {
    const lowered = materializeView(View.spacer(0));
    if (lowered === undefined) throw new Error("native view materialization is unavailable");
    const NativeViewSlot = requireNativeClass(native.NativeViewSlot, "NativeViewSlot");
    super("component", new NativeViewSlot(lowered as object));
  }
  async view(): Promise<View> { this.ensureOpen(); return View.component(this); }
  capabilities(): Promise<ComponentCapabilities> { return this.call(() => ({})); }
  revision(): Promise<number> { return this.call(() => this.nativeHandle.revision()); }
  nativeComponentId(): number | undefined {
    const id = this.nativeHandle.componentId();
    return id === null ? undefined : id;
  }
}

import { HandleBase, requireNativeClass } from "./handles.ts";
import { native } from "../native.ts";
import type { Component as ComponentContract, ComponentCapabilities, ViewSlot as ViewSlotContract } from "./types.ts";
import { nodeForBridge, View } from "./values/view.ts";

type NativeViewSlotHandle = {
  dispose(): void;
  revision(): number;
  componentId(): number | null;
  setView(view: object): void;
  setAnimation(frames: object[], intervalMs: number): void;
  setAnimationAtCycleBoundary(frames: object[], intervalMs: number): void;
  stopAnimation(view: object): void;
};

export class ViewSlot extends HandleBase<NativeViewSlotHandle, "component"> implements ViewSlotContract {
  constructor(nativeHandle: NativeViewSlotHandle | object) { super("component", nativeHandle as NativeViewSlotHandle); }

  view(): View {
    this.ensureOpen();
    return this.nativeComponentId() === undefined ? View.spacer(0) : View.component(this);
  }
  capabilities(): ComponentCapabilities { return this.call(() => ({})); }
  revision(): number { return this.call(() => this.nativeHandle.revision()); }
  setView(view: View): void {
    this.call(() => this.nativeHandle.setView(nodeForBridge(view)));
  }
  setAnimation(frames: readonly View[], intervalMs: number): void {
    this.call(() => {
      if (frames.length === 0) throw new Error("native view slot animation requires at least one frame");
      this.nativeHandle.setAnimation(frames.map(nodeForBridge), intervalMs);
    });
  }
  setAnimationAtCycleBoundary(frames: readonly View[], intervalMs: number): void {
    this.call(() => {
      if (frames.length === 0) throw new Error("native view slot animation requires at least one frame");
      this.nativeHandle.setAnimationAtCycleBoundary(frames.map(nodeForBridge), intervalMs);
    });
  }
  stopAnimation(view: View): void {
    this.call(() => this.nativeHandle.stopAnimation(nodeForBridge(view)));
  }
  nativeComponentId(): number | undefined {
    const id = this.nativeHandle.componentId();
    return id === null ? undefined : id;
  }
}

export class Component extends HandleBase<NativeViewSlotHandle, "component"> implements ComponentContract {
  constructor() {
    const NativeViewSlot = requireNativeClass(native.NativeViewSlot, "NativeViewSlot");
    super("component", new NativeViewSlot(nodeForBridge(View.spacer(0))));
  }
  view(): View {
    this.ensureOpen();
    return this.nativeComponentId() === undefined ? View.spacer(0) : View.component(this);
  }
  capabilities(): ComponentCapabilities { return this.call(() => ({})); }
  revision(): number { return this.call(() => this.nativeHandle.revision()); }
  nativeComponentId(): number | undefined {
    const id = this.nativeHandle.componentId();
    return id === null ? undefined : id;
  }
}

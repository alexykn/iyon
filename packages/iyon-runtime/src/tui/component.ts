import { HandleBase, nativeTui } from "./handles.ts";
import type { Component as ComponentContract, ComponentCapabilities } from "./types.ts";
import { View } from "./values/view.ts";

export class Component extends HandleBase<ReturnType<typeof nativeTui.component>, "component"> implements ComponentContract {
  constructor() { super("component", nativeTui.component()); }
  async view(): Promise<View> { this.ensureOpen(); return View.component(this); }
  capabilities(): Promise<ComponentCapabilities> { return this.call(() => ({ focusable: true })); }
  revision(): Promise<number> { return this.call(() => this.nativeHandle.revision()); }
}

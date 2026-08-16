import type { NativeHandleId } from "./types.ts";
import { HandleBase } from "./handles.ts";

type NativeWorkingHandle = {
  dispose(): void;
  componentId(): number | null;
  setActive(active: boolean): void;
  setPending(pending: string[]): void;
};

export class WorkingActivity extends HandleBase<NativeWorkingHandle, "component"> {
  constructor(nativeHandle: NativeWorkingHandle) { super("component", nativeHandle); }

  setActive(active: boolean): Promise<void> { return this.call(() => this.nativeHandle.setActive(active)); }
  setPending(pending: readonly string[]): Promise<void> { return this.call(() => this.nativeHandle.setPending([...pending])); }
  nativeComponentId(): NativeHandleId | undefined {
    const id = this.nativeHandle.componentId();
    return id === null ? undefined : id as NativeHandleId;
  }
}

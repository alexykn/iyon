import { HandleBase } from "./handles.ts";
import { native } from "../native.ts";
import type { ComponentCapabilities, ScrollPane as ScrollPaneContract } from "./types.ts";
import {
  nativeViewAbiSession,
  releaseNativeViewRef,
  tryNativeMaterialize,
  tryNativeViewBoundaryRender,
} from "./native_view_abi.ts";
import { nodeForBridge, View } from "./values/view.ts";

type NativeScrollPaneHandle = {
  dispose(): void;
  componentId(): number | null;
  setContent(view: object): void;
  setContentRef(viewRef: number): void;
  followEnd(): void;
};

export class NativeScrollPane extends HandleBase<NativeScrollPaneHandle, "component"> implements ScrollPaneContract {
  private currentView?: View;

  constructor(nativeHandle: NativeScrollPaneHandle | object, initialView?: View) {
    super("component", nativeHandle as NativeScrollPaneHandle);
    this.currentView = initialView;
  }

  tuiViewAbiInstallRef(viewRef: number): void { this.nativeHandle.setContentRef(viewRef); }

  view(): View {
    this.ensureOpen();
    return this.nativeComponentId() === undefined ? View.spacer(0) : View.component(this);
  }

  capabilities(): ComponentCapabilities { return this.call(() => ({ focusable: true, keys: ["up", "down", "pageup", "pagedown", "home", "end"] })); }

  setContent(view: View): void {
    this.call(() => {
      const previous = this.currentView;
      if (previous !== undefined) {
        let previousRef = tryNativeMaterialize(previous);
        try {
          const nextRef = tryNativeViewBoundaryRender(this, previous, view, previousRef);
          if (nextRef !== undefined) {
            releaseNativeViewRef(nativeViewAbiSession(), nextRef);
            if (previousRef !== undefined) {
              const retainedRef = previousRef;
              previousRef = undefined;
              releaseNativeViewRef(nativeViewAbiSession(), retainedRef);
            }
            this.currentView = view;
            return;
          }
        } finally {
          if (previousRef !== undefined) {
            const retainedRef = previousRef;
            previousRef = undefined;
            releaseNativeViewRef(nativeViewAbiSession(), retainedRef);
          }
        }
      }
      const ref = tryNativeMaterialize(view);
      if (ref !== undefined) {
        try {
          this.nativeHandle.setContentRef(ref);
          this.currentView = view;
          return;
        } finally {
          releaseNativeViewRef(nativeViewAbiSession(), ref);
        }
      }
      this.nativeHandle.setContent(nodeForBridge(view));
      this.currentView = view;
    });
  }

  followEnd(): void { this.call(() => this.nativeHandle.followEnd()); }

  nativeComponentId(): number | undefined {
    const id = this.nativeHandle.componentId();
    return id === null ? undefined : id;
  }
}

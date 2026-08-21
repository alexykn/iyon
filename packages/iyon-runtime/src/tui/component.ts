import { HandleBase, requireNativeClass } from "./handles.ts";
import { native } from "../native.ts";
import type { Component as ComponentContract, ComponentCapabilities, ViewSlot as ViewSlotContract } from "./types.ts";
import {
  nativeViewAbiSession,
  releaseNativeViewRef,
  tryNativeMaterialize,
  tryNativeViewBoundaryRender,
} from "./native_view_abi.ts";
import { nodeForBridge, View } from "./values/view.ts";

const ANIMATION_REF_SCRATCH = new WeakMap<object, Uint32Array>();

type NativeViewSlotHandle = {
  dispose(): void;
  revision(): number;
  componentId(): number | null;
  setView(view: object): void;
  setViewRef(viewRef: number): void;
  setAnimation(frames: object[], intervalMs: number): void;
  setAnimationAtCycleBoundary(frames: object[], intervalMs: number): void;
  setAnimationRefs(refs: Uint32Array, usedCount: number, intervalMs: number): void;
  setAnimationRefsAtCycleBoundary(refs: Uint32Array, usedCount: number, intervalMs: number): void;
  stopAnimation(view: object): void;
  stopAnimationRef(viewRef: number): void;
};

export class ViewSlot extends HandleBase<NativeViewSlotHandle, "component"> implements ViewSlotContract {
  private currentView?: View;

  constructor(nativeHandle: NativeViewSlotHandle | object, initialView?: View) {
    super("component", nativeHandle as NativeViewSlotHandle);
    this.currentView = initialView;
  }

  tuiViewAbiInstallRef(viewRef: number): void { this.nativeHandle.setViewRef(viewRef); }

  view(): View {
    this.ensureOpen();
    return this.nativeComponentId() === undefined ? View.spacer(0) : View.component(this);
  }
  capabilities(): ComponentCapabilities { return this.call(() => ({})); }
  revision(): number { return this.call(() => this.nativeHandle.revision()); }
  setView(view: View): void {
    this.call(() => {
      const previous = this.currentView;
      if (previous !== undefined) {
        const previousRef = tryNativeMaterialize(previous);
        const nextRef = tryNativeViewBoundaryRender(this, previous, view, previousRef);
        if (nextRef !== undefined) {
          releaseNativeViewRef(nativeViewAbiSession(), nextRef);
          if (previousRef !== undefined) releaseNativeViewRef(nativeViewAbiSession(), previousRef);
          this.currentView = view;
          return;
        }
        if (previousRef !== undefined) releaseNativeViewRef(nativeViewAbiSession(), previousRef);
      }
      const ref = tryNativeMaterialize(view);
      if (ref !== undefined) {
        try {
          this.nativeHandle.setViewRef(ref);
          this.currentView = view;
          return;
        } finally {
          releaseNativeViewRef(nativeViewAbiSession(), ref);
        }
      }
      this.nativeHandle.setView(nodeForBridge(view));
      this.currentView = view;
    });
  }
  setAnimation(frames: readonly View[], intervalMs: number): void {
    this.setAnimationWithRefs(frames, intervalMs, false);
  }
  setAnimationAtCycleBoundary(frames: readonly View[], intervalMs: number): void {
    this.setAnimationWithRefs(frames, intervalMs, true);
  }
  private setAnimationWithRefs(frames: readonly View[], intervalMs: number, atCycleBoundary: boolean): void {
    this.call(() => {
      if (frames.length === 0) throw new Error("native view slot animation requires at least one frame");
      const refs: number[] = [];
      try {
        for (const frame of frames) {
          const ref = tryNativeMaterialize(frame);
          if (ref === undefined) {
            for (const held of refs) releaseNativeViewRef(nativeViewAbiSession(), held);
            this.setAnimationBridge(frames, intervalMs, atCycleBoundary);
            return;
          }
          refs.push(ref);
        }
        let scratch = ANIMATION_REF_SCRATCH.get(this.nativeHandle as object);
        if (scratch === undefined || scratch.length < refs.length) {
          scratch = new Uint32Array(Math.max(refs.length, 4));
          ANIMATION_REF_SCRATCH.set(this.nativeHandle as object, scratch);
        }
        scratch.set(refs);
        if (atCycleBoundary) this.nativeHandle.setAnimationRefsAtCycleBoundary(scratch, refs.length, intervalMs);
        else this.nativeHandle.setAnimationRefs(scratch, refs.length, intervalMs);
      } finally {
        for (const ref of refs) releaseNativeViewRef(nativeViewAbiSession(), ref);
      }
    });
  }
  private setAnimationBridge(frames: readonly View[], intervalMs: number, atCycleBoundary: boolean): void {
    if (atCycleBoundary) this.nativeHandle.setAnimationAtCycleBoundary(frames.map(nodeForBridge), intervalMs);
    else this.nativeHandle.setAnimation(frames.map(nodeForBridge), intervalMs);
  }
  stopAnimation(view: View): void {
    this.call(() => {
      const ref = tryNativeMaterialize(view);
      if (ref !== undefined) {
        try {
          this.nativeHandle.stopAnimationRef(ref);
          return;
        } finally {
          releaseNativeViewRef(nativeViewAbiSession(), ref);
        }
      }
      this.nativeHandle.stopAnimation(nodeForBridge(view));
    });
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

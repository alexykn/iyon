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
  setAnimationRef1?(ref0: number, intervalMs: number): void;
  setAnimationRef2?(ref0: number, ref1: number, intervalMs: number): void;
  setAnimationRef3?(ref0: number, ref1: number, ref2: number, intervalMs: number): void;
  setAnimationRef4?(ref0: number, ref1: number, ref2: number, ref3: number, intervalMs: number): void;
  setAnimationRef1AtCycleBoundary?(ref0: number, intervalMs: number): void;
  setAnimationRef2AtCycleBoundary?(ref0: number, ref1: number, intervalMs: number): void;
  setAnimationRef3AtCycleBoundary?(ref0: number, ref1: number, ref2: number, intervalMs: number): void;
  setAnimationRef4AtCycleBoundary?(ref0: number, ref1: number, ref2: number, ref3: number, intervalMs: number): void;
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
      // Small animations can stay scalar. Large animations write acquired refs
      // directly into the reusable native buffer; do not stage a second JS
      // number[] copy of the frame list.
      const scalarRefs: number[] | undefined = frames.length <= 4 ? [] : undefined;
      let scratch: Uint32Array | undefined;
      let acquiredCount = 0;
      try {
        if (scalarRefs === undefined) {
          scratch = this.animationScratch(frames.length);
        }
        for (const [index, frame] of frames.entries()) {
          const ref = tryNativeMaterialize(frame);
          if (ref === undefined) {
            this.setAnimationBridge(frames, intervalMs, atCycleBoundary);
            return;
          }
          if (scratch !== undefined) scratch[index] = ref;
          else scalarRefs!.push(ref);
          acquiredCount += 1;
        }
        if (scalarRefs !== undefined && this.setFixedAnimationRefs(scalarRefs, intervalMs, atCycleBoundary)) return;
        if (scratch === undefined) {
          scratch = this.animationScratch(scalarRefs!.length);
          scratch.set(scalarRefs!);
        }
        if (atCycleBoundary) this.nativeHandle.setAnimationRefsAtCycleBoundary(scratch, acquiredCount, intervalMs);
        else this.nativeHandle.setAnimationRefs(scratch, acquiredCount, intervalMs);
      } finally {
        if (scratch !== undefined) {
          for (let index = 0; index < acquiredCount; index += 1) releaseNativeViewRef(nativeViewAbiSession(), scratch[index]!);
        } else {
          for (const ref of scalarRefs ?? []) releaseNativeViewRef(nativeViewAbiSession(), ref);
        }
      }
    });
  }

  private animationScratch(requiredLength: number): Uint32Array {
    let scratch = ANIMATION_REF_SCRATCH.get(this.nativeHandle as object);
    if (scratch === undefined || scratch.length < requiredLength) {
      scratch = new Uint32Array(Math.max(requiredLength, 4));
      ANIMATION_REF_SCRATCH.set(this.nativeHandle as object, scratch);
    }
    return scratch;
  }
  private setAnimationBridge(frames: readonly View[], intervalMs: number, atCycleBoundary: boolean): void {
    if (atCycleBoundary) this.nativeHandle.setAnimationAtCycleBoundary(frames.map(nodeForBridge), intervalMs);
    else this.nativeHandle.setAnimation(frames.map(nodeForBridge), intervalMs);
  }

  private setFixedAnimationRefs(refs: readonly number[], intervalMs: number, atCycleBoundary: boolean): boolean {
    if (atCycleBoundary) {
      switch (refs.length) {
        case 1:
          if (this.nativeHandle.setAnimationRef1AtCycleBoundary === undefined) return false;
          this.nativeHandle.setAnimationRef1AtCycleBoundary(refs[0]!, intervalMs);
          return true;
        case 2:
          if (this.nativeHandle.setAnimationRef2AtCycleBoundary === undefined) return false;
          this.nativeHandle.setAnimationRef2AtCycleBoundary(refs[0]!, refs[1]!, intervalMs);
          return true;
        case 3:
          if (this.nativeHandle.setAnimationRef3AtCycleBoundary === undefined) return false;
          this.nativeHandle.setAnimationRef3AtCycleBoundary(refs[0]!, refs[1]!, refs[2]!, intervalMs);
          return true;
        case 4:
          if (this.nativeHandle.setAnimationRef4AtCycleBoundary === undefined) return false;
          this.nativeHandle.setAnimationRef4AtCycleBoundary(refs[0]!, refs[1]!, refs[2]!, refs[3]!, intervalMs);
          return true;
      }
      return false;
    }
    switch (refs.length) {
      case 1:
        if (this.nativeHandle.setAnimationRef1 === undefined) return false;
        this.nativeHandle.setAnimationRef1(refs[0]!, intervalMs);
        return true;
      case 2:
        if (this.nativeHandle.setAnimationRef2 === undefined) return false;
        this.nativeHandle.setAnimationRef2(refs[0]!, refs[1]!, intervalMs);
        return true;
      case 3:
        if (this.nativeHandle.setAnimationRef3 === undefined) return false;
        this.nativeHandle.setAnimationRef3(refs[0]!, refs[1]!, refs[2]!, intervalMs);
        return true;
      case 4:
        if (this.nativeHandle.setAnimationRef4 === undefined) return false;
        this.nativeHandle.setAnimationRef4(refs[0]!, refs[1]!, refs[2]!, refs[3]!, intervalMs);
        return true;
      default:
        return false;
    }
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

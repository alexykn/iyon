import type { Pointer } from "bun:ffi";
import { native, type NativeViewAbiBootstrap } from "../native.ts";
import { linkViewAbi, type NativeAbiPointers } from "./generated/view_abi.ts";
import {
  hostRenderRef,
  viewCommonPatchRoot,
  viewRefForNodeId,
  viewReleaseMany,
  viewTextLayoutPatchRoot,
  type ViewAbiSymbols,
} from "./generated/view_calls.ts";
import {
  BRIDGE_VIEW_KIND,
  type BridgeViewNode,
  type DecorationNode,
} from "./ir.ts";
import { nodeForBridge, nodeIdPair, type View } from "./values/view.ts";
import manifest from "./generated/view_abi_manifest.json";

export interface NativeViewAbiSession {
  readonly runtime: Pointer;
  readonly symbols: ViewAbiSymbols;
  readonly abi: NativeViewAbiBootstrap;
}

export interface NativeViewRenderHost {
  readonly tuiViewAbiHostPointer?: () => number;
}

let cachedSession: NativeViewAbiSession | undefined;

/**
 * Links the generated first-slice ABI once for this Bun environment.
 * The native runtime pointer is environment-owned and remains stable until
 * N-API cleanup; callers must not retain the session after addon teardown.
 */
export function nativeViewAbiSession(): NativeViewAbiSession | undefined {
  if (cachedSession !== undefined) return cachedSession;
  const bootstrap = native.tuiViewAbiBootstrap?.();
  if (bootstrap === undefined) return undefined;
  const rawPointers = Object.values(bootstrap.functions);
  if (
    bootstrap.abi_name !== "iyon_tui_view"
    || bootstrap.abi_version !== 1
    || bootstrap.semantic_version !== 1
    || bootstrap.schema_blake3 !== manifest.schema_blake3
    || bootstrap.generator_blake3 !== manifest.generator_blake3
    || !Number.isSafeInteger(bootstrap.generation)
    || bootstrap.generation < 1
    || bootstrap.function_count !== manifest.functions.length
    || !Number.isSafeInteger(bootstrap.runtime_ptr)
    || bootstrap.runtime_ptr <= 0
    || rawPointers.some((pointer) => !Number.isSafeInteger(pointer) || pointer <= 0)
  ) {
    throw new Error("native View ABI bootstrap metadata is incompatible");
  }
  const pointers: NativeAbiPointers = {
    runtimeNoop: bootstrap.functions.runtimeNoop as Pointer,
    viewRenderRef: bootstrap.functions.viewRenderRef as Pointer,
    hostRenderRef: bootstrap.functions.hostRenderRef as Pointer,
    viewSpacerCreate: bootstrap.functions.viewSpacerCreate as Pointer,
    viewTextLayoutPatchRoot: bootstrap.functions.viewTextLayoutPatchRoot as Pointer,
    viewCommonPatchRoot: bootstrap.functions.viewCommonPatchRoot as Pointer,
    viewAxisCreateBuffer: bootstrap.functions.viewAxisCreateBuffer as Pointer,
    viewReleaseMany: bootstrap.functions.viewReleaseMany as Pointer,
    viewRefForNodeId: bootstrap.functions.viewRefForNodeId as Pointer,
  };
  const linked = linkViewAbi(pointers);
  const runtime = bootstrap.runtime_ptr as Pointer;
  if (linked.symbols.runtimeNoop(runtime) !== 1) {
    throw new Error("native View ABI bootstrap probe failed");
  }
  cachedSession = {
    runtime,
    symbols: linked.symbols,
    abi: bootstrap,
  };
  return cachedSession;
}

/**
 * Obtains the environment-local NativeRef after an authoritative direct
 * decode has installed the corresponding View. The caller owns the resulting
 * root lease until it replaces or closes that root.
 */
export function nativeViewRefForNodeId(view: View): number | undefined {
  const session = nativeViewAbiSession();
  if (session === undefined) return undefined;
  const [nodeIdLow, nodeIdHigh] = nodeIdPair(view);
  return viewRefForNodeId(session.symbols, session.runtime, nodeIdLow, nodeIdHigh);
}

/**
 * Attempts the Tranche 5 scalar retained route. Unsupported lineage remains
 * on the existing direct/V4 fallback; no bridge node or command record is
 * constructed by this function.
 */
export function tryNativeScalarRender(
  host: NativeViewRenderHost,
  previous: View,
  previousRef: number,
  next: View,
): number | undefined {
  const hostPointer = host.tuiViewAbiHostPointer?.();
  if (hostPointer === undefined || hostPointer <= 0 || previousRef <= 0) return undefined;
  const session = nativeViewAbiSession();
  if (session === undefined) return undefined;

  const hostPtr = hostPointer as Pointer;
  const previousNode = nodeForBridge(previous);
  const nextNode = nodeForBridge(next);
  let nextRef: number | undefined;
  try {
    nextRef = tryTextLayoutPatch(session, previousNode, nextNode, previousRef, next);
    if (nextRef === undefined) {
      nextRef = tryCommonPatch(session, previousNode, nextNode, previousRef, next);
    }
    if (nextRef === undefined) return undefined;
    const status = hostRenderRef(session.symbols, session.runtime, hostPtr, nextRef);
    if (status !== 0) {
      releaseNativeViewRefs(session, [nextRef]);
      return undefined;
    }
    return nextRef;
  } catch {
    if (nextRef !== undefined) releaseNativeViewRefs(session, [nextRef]);
    return undefined;
  }
}

export function releaseNativeViewRefs(session: NativeViewAbiSession | undefined, refs: readonly number[]): void {
  if (session === undefined || refs.length === 0) return;
  const values = new Uint32Array(refs.length);
  values.set(refs);
  viewReleaseMany(session.symbols, session.runtime, values, values.length);
}

function tryTextLayoutPatch(
  session: NativeViewAbiSession,
  previousNode: BridgeViewNode,
  nextNode: BridgeViewNode,
  previousRef: number,
  next: View,
): number | undefined {
  if (previousNode.kind !== BRIDGE_VIEW_KIND.text || nextNode.kind !== BRIDGE_VIEW_KIND.text) return undefined;
  if (previousNode.spans !== nextNode.spans) return undefined;
  if (previousNode.wrap === nextNode.wrap && previousNode.align === nextNode.align) return undefined;
  const [nodeIdLow, nodeIdHigh] = nodeIdPair(next);
  return viewTextLayoutPatchRoot(
    session.symbols,
    session.runtime,
    previousRef,
    nodeIdLow,
    nodeIdHigh,
    nextNode.wrap,
    nextNode.align,
  );
}

function tryCommonPatch(
  session: NativeViewAbiSession,
  previousNode: BridgeViewNode,
  nextNode: BridgeViewNode,
  previousRef: number,
  next: View,
): number | undefined {
  if (nextNode.kind !== BRIDGE_VIEW_KIND.decorated || nextNode.child.id !== previousNode.id) return undefined;
  const decoration = nextNode.decoration;
  if (!isScalarDecoration(decoration)) return undefined;

  let mask = 0;
  let paddingTopRight = 0;
  let paddingBottomLeft = 0;
  if (decoration.padding !== undefined) {
    mask |= 4;
    paddingTopRight = packInsets(decoration.padding.top, decoration.padding.right);
    paddingBottomLeft = packInsets(decoration.padding.bottom, decoration.padding.left);
  }
  if (decoration.width !== undefined) mask |= 8;
  if (decoration.height !== undefined) mask |= 16;
  if (decoration.minWidth !== undefined) mask |= 32;
  if (decoration.maxWidth !== undefined) mask |= 64;
  if (decoration.minHeight !== undefined) mask |= 128;
  if (decoration.maxHeight !== undefined) mask |= 256;
  if (mask === 0) return undefined;

  const [nodeIdLow, nodeIdHigh] = nodeIdPair(next);
  return viewCommonPatchRoot(
    session.symbols,
    session.runtime,
    previousRef,
    nodeIdLow,
    nodeIdHigh,
    mask,
    paddingTopRight,
    paddingBottomLeft,
    decoration.width === "fit" ? 1 : decoration.width === "fill" ? 2 : 0,
    decoration.height === "fit" ? 1 : decoration.height === "fill" ? 2 : 0,
    decoration.minWidth ?? 0,
    decoration.maxWidth ?? 0,
    decoration.minHeight ?? 0,
    decoration.maxHeight ?? 0,
    previousRef,
  );
}

function isScalarDecoration(decoration: DecorationNode): boolean {
  if (decoration.background !== undefined || decoration.foreground !== undefined || decoration.border !== undefined) return false;
  if (decoration.styleStates !== undefined && Object.keys(decoration.styleStates).length > 0) return false;
  if (decoration.style.theme !== undefined || decoration.style.foreground !== undefined || decoration.style.background !== undefined) return false;
  return Object.keys(decoration.style.attributes).length === 0;
}

function packInsets(first: number, second: number): number {
  return (first | (second << 16)) >>> 0;
}

/** Test-only reset; production sessions are environment-owned and stable. */
export function resetNativeViewAbiSessionForTests(): void {
  cachedSession = undefined;
}

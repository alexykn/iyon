import type { Pointer } from "bun:ffi";
import { native, type NativeViewAbiBootstrap } from "../native.ts";
import { linkViewAbi, type NativeAbiPointers } from "./generated/view_abi.ts";
import {
  hostRenderRef,
  pathChild,
  pathRoot,
  viewCommonPatchRoot,
  viewRefForNodeId,
  viewReleaseMany,
  viewTextLayoutPatchPathD1,
  viewTextLayoutPatchPathD2,
  viewTextLayoutPatchPathD3,
  viewTextLayoutPatchPathD4,
  viewTextLayoutPatchRoot,
  type ViewAbiSymbols,
} from "./generated/view_calls.ts";
import {
  BRIDGE_VIEW_KIND,
  type BridgeViewNode,
  type DecorationNode,
} from "./ir.ts";
import {
  nativePathLineage,
  nodeForBridge,
  nodeIdPair,
  type NativePathLineage,
  type View,
} from "./values/view.ts";
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
const PATH_REFS = new WeakMap<NativeViewAbiSession, WeakMap<object, number>>();
const PATH_SHAPE_REFS = new WeakMap<NativeViewAbiSession, Map<string, number>>();
const SINGLE_REF_RELEASE = new Uint32Array(1);
const ABI_FUNCTION_NAMES = [
  "runtimeNoop",
  "viewRenderRef",
  "hostRenderRef",
  "viewSpacerCreate",
  "viewTextLayoutPatchRoot",
  "viewCommonPatchRoot",
  "viewAxisCreateBuffer",
  "viewReleaseMany",
  "viewRefForNodeId",
  "pathRoot",
  "pathChild",
  "viewTextLayoutPatchPath",
  "viewTextLayoutPatchPathD1",
  "viewTextLayoutPatchPathD2",
  "viewTextLayoutPatchPathD3",
  "viewTextLayoutPatchPathD4",
] as const;

function isValidPointer(value: unknown): value is Pointer {
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0;
}

/**
 * Links the generated first-slice ABI once for this Bun environment.
 * The native runtime pointer is environment-owned and remains stable until
 * N-API cleanup; callers must not retain the session after addon teardown.
 */
export function nativeViewAbiSession(): NativeViewAbiSession | undefined {
  if (cachedSession !== undefined) return cachedSession;
  const bootstrap = native.tuiViewAbiBootstrap?.();
  if (bootstrap === undefined) return undefined;
  const functionsValue = bootstrap.functions as unknown;
  if (functionsValue === null || typeof functionsValue !== "object") {
    throw new Error("native View ABI bootstrap metadata is incompatible");
  }
  const functions = functionsValue as Record<string, unknown>;
  const functionNames = Object.keys(functions);
  if (
    bootstrap.abi_name !== "iyon_tui_view"
    || bootstrap.abi_version !== 1
    || bootstrap.semantic_version !== 1
    || bootstrap.schema_blake3 !== manifest.schema_blake3
    || bootstrap.generator_blake3 !== manifest.generator_blake3
    || !Number.isSafeInteger(bootstrap.generation)
    || bootstrap.generation < 1
    || bootstrap.function_count !== manifest.functions.length
    || functionNames.length !== ABI_FUNCTION_NAMES.length
    || ABI_FUNCTION_NAMES.some((name) => !functionNames.includes(name) || !isValidPointer(functions[name]))
    || !isValidPointer(bootstrap.runtime_ptr)
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
    pathRoot: bootstrap.functions.pathRoot as Pointer,
    pathChild: bootstrap.functions.pathChild as Pointer,
    viewTextLayoutPatchPath: bootstrap.functions.viewTextLayoutPatchPath as Pointer,
    viewTextLayoutPatchPathD1: bootstrap.functions.viewTextLayoutPatchPathD1 as Pointer,
    viewTextLayoutPatchPathD2: bootstrap.functions.viewTextLayoutPatchPathD2 as Pointer,
    viewTextLayoutPatchPathD3: bootstrap.functions.viewTextLayoutPatchPathD3 as Pointer,
    viewTextLayoutPatchPathD4: bootstrap.functions.viewTextLayoutPatchPathD4 as Pointer,
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
  if (!isValidPointer(hostPointer) || !isValidNativeRef(previousRef)) return undefined;
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
      releaseNativeViewRef(session, nextRef);
      return undefined;
    }
    return nextRef;
  } catch (error) {
    if (nextRef !== undefined) releaseNativeViewRef(session, nextRef);
    if (isExpectedNativeStatus(error)) return undefined;
    throw error;
  }
}

/** Interns one immutable JS path lineage into the environment PathStore. */
export function nativePathRefForLineage(
  session: NativeViewAbiSession,
  lineage: NativePathLineage,
): number {
  let refs = PATH_REFS.get(session);
  if (refs === undefined) {
    refs = new WeakMap<object, number>();
    PATH_REFS.set(session, refs);
  }
  const cached = refs.get(lineage);
  if (cached !== undefined) return cached;
  let shapes = PATH_SHAPE_REFS.get(session);
  if (shapes === undefined) {
    shapes = new Map<string, number>();
    PATH_SHAPE_REFS.set(session, shapes);
  }
  const shape = pathShape(lineage);
  const shaped = shapes.get(shape);
  if (shaped !== undefined) {
    refs.set(lineage, shaped);
    return shaped;
  }
  const parent = lineage.parent === undefined
    ? pathRoot(session.symbols, session.runtime)
    : nativePathRefForLineage(session, lineage.parent);
  const reference = lineage.step === undefined
    ? parent
    : pathChild(
      session.symbols,
      session.runtime,
      parent,
      lineage.step.kind,
      lineage.step.expectedViewKind,
      lineage.step.selector,
    );
  shapes.set(shape, reference);
  refs.set(lineage, reference);
  return reference;
}

/** Routes a shallow retained text edit through a cached native PathRef. */
export function tryNativePathScalarRender(
  host: NativeViewRenderHost,
  previous: View,
  previousRef: number,
  next: View,
): number | undefined {
  const lineage = nativePathLineage(next);
  if (lineage === undefined || lineage.baseNodeId !== nodeForBridge(previous).id || lineage.depth < 1 || lineage.depth > 4) return undefined;
  const hostPointer = host.tuiViewAbiHostPointer?.();
  if (!isValidPointer(hostPointer) || !isValidNativeRef(previousRef)) return undefined;
  const session = nativeViewAbiSession();
  if (session === undefined) return undefined;
  const steps = pathLineageSteps(lineage);
  const nodes = pathNodes(nodeForBridge(next), steps);
  if (nodes === undefined || nodes.length !== lineage.depth + 1) return undefined;
  const target = splitNodeId(nodes[nodes.length - 1]!.id);
  const ancestors = nodes.slice(0, -1).reverse().map((node) => splitNodeId(node.id));
  const targetNode = nodes[nodes.length - 1]!;
  if (targetNode.kind !== BRIDGE_VIEW_KIND.text) return undefined;
  const wrap = targetNode.wrap;
  const align = targetNode.align;
  let nextRef: number | undefined;
  try {
    const pathRef = nativePathRefForLineage(session, lineage);
    nextRef = lineage.depth === 1
      ? viewTextLayoutPatchPathD1(session.symbols, session.runtime, previousRef, pathRef, target[0], target[1], ancestors[0]![0], ancestors[0]![1], wrap, align)
      : lineage.depth === 2
        ? viewTextLayoutPatchPathD2(session.symbols, session.runtime, previousRef, pathRef, target[0], target[1], ancestors[0]![0], ancestors[0]![1], ancestors[1]![0], ancestors[1]![1], wrap, align)
        : lineage.depth === 3
          ? viewTextLayoutPatchPathD3(session.symbols, session.runtime, previousRef, pathRef, target[0], target[1], ancestors[0]![0], ancestors[0]![1], ancestors[1]![0], ancestors[1]![1], ancestors[2]![0], ancestors[2]![1], wrap, align)
          : viewTextLayoutPatchPathD4(session.symbols, session.runtime, previousRef, pathRef, target[0], target[1], ancestors[0]![0], ancestors[0]![1], ancestors[1]![0], ancestors[1]![1], ancestors[2]![0], ancestors[2]![1], ancestors[3]![0], ancestors[3]![1], wrap, align);
    const status = hostRenderRef(session.symbols, session.runtime, hostPointer as Pointer, nextRef);
    if (status !== 0) {
      releaseNativeViewRef(session, nextRef);
      return undefined;
    }
    return nextRef;
  } catch (error) {
    if (nextRef !== undefined) releaseNativeViewRef(session, nextRef);
    if (isExpectedNativeStatus(error)) return undefined;
    throw error;
  }
}

function pathShape(lineage: NativePathLineage): string {
  const steps = pathLineageSteps(lineage);
  return steps.length === 0 ? "root" : steps.map((step) => `${step?.kind}:${step?.expectedViewKind}:${step?.selector}`).join("/");
}

function pathLineageSteps(lineage: NativePathLineage): NativePathLineage["step"][] {
  const steps: NativePathLineage["step"][] = [];
  let current: NativePathLineage | undefined = lineage;
  while (current?.step !== undefined) {
    steps.push(current.step);
    current = current.parent;
  }
  steps.reverse();
  return steps;
}

function pathNodes(root: BridgeViewNode, steps: readonly NativePathLineage["step"][]): BridgeViewNode[] | undefined {
  const nodes = [root];
  let current = root;
  for (const step of steps) {
    if (step === undefined) return undefined;
    switch (step.kind) {
      case 1:
      case 2: {
        if (step.selector !== 0 || (current.kind !== BRIDGE_VIEW_KIND.container && current.kind !== BRIDGE_VIEW_KIND.clamp && current.kind !== BRIDGE_VIEW_KIND.contentMax)) return undefined;
        current = current.child;
        break;
      }
      case 4: {
        if (current.kind !== BRIDGE_VIEW_KIND.column) return undefined;
        const child = current.children[step.selector];
        if (child === undefined) return undefined;
        current = child.child;
        break;
      }
      case 5: {
        if (current.kind !== BRIDGE_VIEW_KIND.row) return undefined;
        const child = current.children[step.selector];
        if (child === undefined) return undefined;
        current = child.child;
        break;
      }
      case 6: {
        if (current.kind !== BRIDGE_VIEW_KIND.grid || step.selector < 0) return undefined;
        let remaining = step.selector;
        let found: BridgeViewNode | undefined;
        for (const row of current.rows) {
          for (const cell of row.cells) {
            if (remaining === 0) found = cell.view;
            remaining -= 1;
          }
        }
        if (found === undefined || remaining >= 0) return undefined;
        current = found;
        break;
      }
      case 7:
      case 8:
      case 9: {
        if (current.kind !== BRIDGE_VIEW_KIND.hanging || step.selector !== 0) return undefined;
        current = step.kind === 7 ? current.prefix : step.kind === 8 ? current.continuation : current.body;
        break;
      }
      default: return undefined;
    }
    nodes.push(current);
  }
  return nodes;
}

function splitNodeId(id: number): readonly [number, number] {
  if (!Number.isSafeInteger(id) || id <= 0 || id > Number.MAX_SAFE_INTEGER) throw new RangeError("native path NodeId is invalid");
  return [id >>> 0, Math.floor(id / 0x1_0000_0000)];
}

export function releaseNativeViewRef(session: NativeViewAbiSession | undefined, ref: number): void {
  if (session === undefined || !isValidNativeRef(ref)) return;
  SINGLE_REF_RELEASE[0] = ref;
  viewReleaseMany(session.symbols, session.runtime, SINGLE_REF_RELEASE, 1);
}

function isValidNativeRef(value: number): boolean {
  return Number.isSafeInteger(value) && value > 0 && value < 0x8000_0000;
}

function isExpectedNativeStatus(error: unknown): boolean {
  return error instanceof Error && /^native ABI status 0x[0-9a-f]+$/u.test(error.message);
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

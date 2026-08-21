import { CFunction, toArrayBuffer, type Pointer } from "bun:ffi";
import {
  BRIDGE_VIEW_KIND,
  PACKED_V3,
  type BridgeGridTrackNode,
  type BridgeLayoutChild,
  type BridgeOverflowIndicatorNode,
  type BridgeViewNode,
  type BridgeViewNodeDraft,
} from "./ir.ts";
import {
  nodeForBridge,
  replaceAxisChildForPackedTransport,
  replaceGridCellForPackedTransport,
  spliceAxisChildrenForPackedTransport,
  type View,
} from "./values/view.ts";
import {
  packedMeta,
  packedSequenceMeta,
  type CanonicalColor,
  type CanonicalDecoration,
  type CanonicalStyle,
  type PackedGridCell,
  type PackedLineage,
} from "./packed_v3_meta.ts";
import { PersistentSeq, type PersistentSeqNode } from "./persistent_seq.ts";
type FastSharedHost = { readonly tuiPerfFastSharedAbi?: () => FastSharedAbi };

const collectCounters = Bun.env.PERF_COUNTERS !== "0";
const FAST_ABI_MAGIC = 0x494f_4654;
const FAST_ABI_VERSION = 1;
const FAST_COMMAND_BYTES = 256 * 1024;
const FAST_META_OFFSET = 128 * 1024;
const FAST_CONTROL_WORDS = 16;
const FAST_OP_WORDS = 10;
const FAST_META_WORD = FAST_META_OFFSET / 4;
const FAST_LOCAL_BIT = 0x8000_0000;
const MAX_GENERATION = 0xffff_ffff;
const OP_DEF_TEXT = 1;
const OP_DEF_DIFF = 2;
const OP_DEF_SPACER = 3;
const OP_DEF_AXIS = 4;
const OP_DEF_HANGING = 5;
const OP_DEF_GRID = 6;
const OP_DEF_CONTAINER = 7;
const OP_DEF_CLAMP = 8;
const OP_DEF_CONTENT_MAX = 9;
const OP_DEF_COMPONENT = 10;
const OP_DEF_DECORATED = 11;
const OP_DEF_SEQ_LEAF = 12;
const OP_DEF_SEQ_BRANCH = 13;
const OP_DEF_GRID_LEAF = 14;
const OP_DEF_GRID_BRANCH = 15;
const OP_PATCH_TEXT = 16;
const OP_PATCH_DECORATION = 17;
const OP_PATCH_AXIS = 18;
const OP_PATCH_GRID = 19;
const FAST_CACHE_MISS = 1;
const FAST_SUPPORTED_BUN_VERSION = "1.4.0";

export type FastSharedAbiPage = { readonly id: number; readonly ptr: number; readonly bytes: number };
export type FastSharedAbi = {
  readonly magic: number;
  readonly version: number;
  readonly schema_version: number;
  readonly control_words: number;
  readonly op_words: number;
  readonly command_bytes: number;
  readonly meta_offset: number;
  readonly max_ops: number;
  readonly page_bytes: number;
  readonly runtime_ptr: number;
  readonly host_ptr: number;
  readonly command_ptr: number;
  readonly pages: readonly FastSharedAbiPage[];
  readonly commit_ptr: number;
  readonly acquire_ptr: number;
  readonly release_ptr: number;
  readonly render_ref_ptr: number;
};

type FastCall = (runtime: Pointer, host: Pointer, ...args: number[]) => number;

type FastHooks = {
  readonly encodingStarted?: () => void;
  readonly encodingFinished?: () => void;
  readonly nativeStarted?: () => void;
  readonly nativeFinished?: () => void;
  readonly cacheMiss?: () => void;
};

export type FastSharedStatus = {
  readonly code: number;
  readonly detail?: number;
};

export type FastSharedCounters = {
  fast_shared_commits: number;
  fast_shared_exact_ref_calls: number;
  fast_shared_ops_emitted: number;
  fast_shared_local_refs: number;
  fast_shared_persistent_refs: number;
  fast_shared_utf8_strings: number;
  fast_shared_utf8_bytes: number;
  fast_shared_page_acquires: number;
  fast_shared_page_reuses: number;
  fast_shared_page_seals: number;
  fast_shared_large_pages: number;
  fast_shared_cache_misses: number;
  fast_shared_v3_fallbacks: number;
};

const counters: FastSharedCounters = {
  fast_shared_commits: 0,
  fast_shared_exact_ref_calls: 0,
  fast_shared_ops_emitted: 0,
  fast_shared_local_refs: 0,
  fast_shared_persistent_refs: 0,
  fast_shared_utf8_strings: 0,
  fast_shared_utf8_bytes: 0,
  fast_shared_page_acquires: 0,
  fast_shared_page_reuses: 0,
  fast_shared_page_seals: 0,
  fast_shared_large_pages: 0,
  fast_shared_cache_misses: 0,
  fast_shared_v3_fallbacks: 0,
};

export function resetFastSharedCounters(): void { for (const key of Object.keys(counters) as (keyof FastSharedCounters)[]) counters[key] = 0; }
export function fastSharedSnapshot(): FastSharedCounters { return { ...counters }; }
export function recordFastSharedFallback(): void { if (collectCounters) counters.fast_shared_v3_fallbacks += 1; }

export class FastSharedError extends Error {
  readonly code: number;
  readonly detail?: number;
  constructor(status: FastSharedStatus) {
    super(`ION_FAST_SHARED_${status.code}`);
    this.name = "FastSharedError";
    this.code = status.code;
    this.detail = status.detail;
  }
}

function callPointer(pointer: number, args: readonly string[]): FastCall {
  return CFunction({ ptr: pointer as Pointer, args: ["ptr", "ptr", ...args] as never, returns: "i32" }) as unknown as FastCall;
}

function validateAbi(abi: FastSharedAbi): void {
  if (Bun.version !== FAST_SUPPORTED_BUN_VERSION) throw new Error(`ION_FAST_SHARED_BUN_MISMATCH:${FAST_SUPPORTED_BUN_VERSION}`);
  if (abi.magic !== FAST_ABI_MAGIC || abi.version !== FAST_ABI_VERSION || abi.schema_version !== 1 || abi.control_words !== FAST_CONTROL_WORDS || abi.op_words !== FAST_OP_WORDS || abi.command_bytes !== FAST_COMMAND_BYTES || abi.meta_offset !== FAST_META_OFFSET) {
    throw new Error("ION_FAST_SHARED_ABI_MISMATCH");
  }
  if (![abi.runtime_ptr, abi.host_ptr].every((value) => Number.isSafeInteger(value) && value > 0) || !Number.isSafeInteger(abi.command_ptr)) throw new Error("ION_FAST_SHARED_HANDLE_INVALID");
  if (abi.pages.length === 0 || abi.pages.some((page) => page.bytes <= 0 || !Number.isSafeInteger(page.ptr))) throw new Error("ION_FAST_SHARED_PAGE_INVALID");
  if (![abi.commit_ptr, abi.acquire_ptr, abi.release_ptr, abi.render_ref_ptr].every((value) => Number.isSafeInteger(value) && value > 0)) throw new Error("ION_FAST_SHARED_FUNCTION_POINTER_INVALID");
}

export class FastSharedTransport {
  readonly abi: FastSharedAbi;
  readonly command: Uint8Array;
  readonly control: Uint32Array;
  readonly meta: Uint32Array;
  private readonly pages: readonly Uint8Array[];
  readonly runtimePtr: Pointer;
  readonly hostPtr: Pointer;
  private host: FastSharedHost | undefined;
  private readonly commitCall: FastCall;
  private readonly acquireCall: FastCall;
  private readonly releaseCall: FastCall;
  private readonly renderRefCall: FastCall;
  private pageId = -1;
  private byteCursor = 0;
  private readonly acquiredPages = new Set<number>();
  private metaCursor = 0;
  private sequence = 0;
  private closed = false;
  private readonly encoder = new TextEncoder();

  constructor(abi: FastSharedAbi, host?: FastSharedHost) {
    validateAbi(abi);
    this.host = host;
    this.abi = abi;
    this.runtimePtr = abi.runtime_ptr as Pointer;
    this.hostPtr = abi.host_ptr as Pointer;
    this.command = new Uint8Array(toArrayBuffer(abi.command_ptr as Pointer, 0, abi.command_bytes));
    this.control = new Uint32Array(this.command.buffer, this.command.byteOffset, FAST_CONTROL_WORDS);
    this.meta = new Uint32Array(this.command.buffer, this.command.byteOffset + FAST_META_OFFSET, (FAST_COMMAND_BYTES - FAST_META_OFFSET) / 4);
    this.pages = abi.pages.map((page) => new Uint8Array(toArrayBuffer(page.ptr as Pointer, 0, page.bytes)));
    this.commitCall = callPointer(abi.commit_ptr, []);
    this.acquireCall = callPointer(abi.acquire_ptr, []);
    this.releaseCall = callPointer(abi.release_ptr, ["u32"]);
    this.renderRefCall = callPointer(abi.render_ref_ptr, ["u32", "u32"]);
  }

  begin(generation: number, flags = 0): void {
    if (this.closed) throw new Error("ION_FAST_SHARED_CLOSED");
    this.control.fill(0);
    this.meta.fill(0, 0, Math.min(this.meta.length, this.metaCursor));
    this.control[0] = FAST_ABI_MAGIC;
    this.control[1] = FAST_ABI_VERSION;
    this.control[2] = generation >>> 0;
    this.control[3] = (++this.sequence) >>> 0;
    this.control[8] = FAST_META_OFFSET;
    this.control[10] = flags >>> 0;
    this.control[13] = 1;
    this.pageId = -1;
    this.byteCursor = 0;
    this.metaCursor = 0;
  }

  emit(opcode: number, destination: number, base: number, nodeId: number, a = 0, b = 0, c = 0, d = 0, e = 0): number {
    const index = this.control[4]!;
    if (index >= this.abi.max_ops) throw new FastSharedError({ code: 6, detail: index });
    const offset = FAST_CONTROL_WORDS * 4 + index * FAST_OP_WORDS * 4;
    const words = new Uint32Array(this.command.buffer, this.command.byteOffset + offset, FAST_OP_WORDS);
    const low = nodeId % 0x1_0000_0000;
    const high = Math.floor(nodeId / 0x1_0000_0000);
    words[0] = opcode >>> 0;
    words[1] = destination >>> 0;
    words[2] = base >>> 0;
    words[3] = low >>> 0;
    words[4] = high >>> 0;
    words[5] = a >>> 0;
    words[6] = b >>> 0;
    words[7] = c >>> 0;
    words[8] = d >>> 0;
    words[9] = e >>> 0;
    this.control[4] = index + 1;
    if (collectCounters) counters.fast_shared_ops_emitted += 1;
    return index;
  }

  local(index: number): number { return (FAST_LOCAL_BIT | index) >>> 0; }
  persistent(ref: number): number { if (!Number.isSafeInteger(ref) || ref <= 0 || ref >= FAST_LOCAL_BIT) throw new RangeError("fast shared persistent ref is invalid"); return ref; }

  payload(build: (writer: FastMetaWriter) => void): readonly [number, number] {
    const start = this.metaCursor;
    const writer = new FastMetaWriter(this);
    build(writer);
    const length = this.metaCursor - start;
    return [FAST_META_WORD + start, length];
  }

  pushMeta(value: number): void {
    if (!Number.isSafeInteger(value) || value < 0 || value > 0xffff_ffff || this.metaCursor >= this.meta.length) throw new FastSharedError({ code: 6, detail: this.metaCursor });
    this.meta[this.metaCursor++] = value >>> 0;
  }

  writeUtf8(value: string): readonly [number, number] {
    if (this.closed) throw new FastSharedError({ code: 2 });
    validateUtf16(value);
    if (value.length === 0) return [0, 0];
    if (this.pageId < 0) {
      const pageId = this.acquireCall(this.runtimePtr, this.hostPtr);
      if (pageId < 0 || pageId >= this.pages.length) throw new FastSharedError({ code: pageId < 0 ? -pageId : 5 });
      this.pageId = pageId;
      this.byteCursor = 0;
      if (collectCounters) {
        counters.fast_shared_page_acquires += 1;
        if (this.acquiredPages.has(pageId)) counters.fast_shared_page_reuses += 1;
        this.acquiredPages.add(pageId);
      }
    }
    const page = this.pages[this.pageId]!;
    const start = this.byteCursor;
    const result = this.encoder.encodeInto(value, page.subarray(start));
    if (result.read !== value.length) throw new FastSharedError({ code: 6, detail: value.length });
    this.byteCursor += result.written;
    if (collectCounters) counters.fast_shared_utf8_strings += 1;
    if (collectCounters) counters.fast_shared_utf8_bytes += result.written;
    return [start, result.written];
  }

  commit(root: number): FastSharedStatus {
    if (this.closed) throw new FastSharedError({ code: 2 });
    this.control[5] = root >>> 0;
    this.control[6] = this.pageId < 0 ? 0 : this.pageId;
    this.control[7] = this.byteCursor;
    this.control[9] = this.metaCursor * 4;
    if (collectCounters) counters.fast_shared_commits += 1;
    const code = this.commitCall(this.runtimePtr, this.hostPtr);
    if (code !== 0) throw new FastSharedError({ code, detail: this.control[12] });
    if (collectCounters && this.pageId >= 0) counters.fast_shared_page_seals += 1;
    this.pageId = -1;
    this.byteCursor = 0;
    return { code };
  }

  renderRef(generation: number, reference: number): FastSharedStatus {
    if (this.closed) throw new FastSharedError({ code: 2 });
    const code = this.renderRefCall(this.runtimePtr, this.hostPtr, generation >>> 0, reference >>> 0);
    if (code !== 0) throw new FastSharedError({ code });
    return { code };
  }

  releaseWritingPage(): void {
    if (this.closed) return;
    if (this.pageId >= 0) this.releaseCall(this.runtimePtr, this.hostPtr, this.pageId);
    this.pageId = -1;
    this.byteCursor = 0;
  }

  close(): void {
    if (this.closed) return;
    this.closed = true;
    (this.commitCall as unknown as { close?: () => void }).close?.();
    (this.acquireCall as unknown as { close?: () => void }).close?.();
    (this.releaseCall as unknown as { close?: () => void }).close?.();
    (this.renderRefCall as unknown as { close?: () => void }).close?.();
    this.host = undefined;
  }
}

class FastMetaWriter {
  constructor(private readonly target: FastSharedTransport) {}
  push(value: number): void { this.target.pushMeta(value); }
  string(value: string): void { const [offset, length] = this.target.writeUtf8(value); this.push(offset); this.push(length); }
  u64(value: number): void { if (!Number.isSafeInteger(value) || value < 0) throw new RangeError("fast shared u64 is invalid"); this.push(value % 0x1_0000_0000); this.push(Math.floor(value / 0x1_0000_0000)); }
  color(color: CanonicalColor): void { if (color.kind === "string") { this.push(1); this.string(color.value); } else { this.push(2); this.push(color.value); } }
  style(style: CanonicalStyle | undefined): void {
    if (style === undefined) { this.push(0); this.push(0); this.push(0); }
    else {
      this.push(style.flags); if (style.flags & 1) this.string(style.theme!); if (style.flags & 2) this.color(style.foreground!); if (style.flags & 4) this.color(style.background!); this.push(style.attributePresent); this.push(style.attributeTrue);
    }
  }
  track(track: BridgeGridTrackNode): void { this.push(track.kind); this.push("max" in track ? track.max : "size" in track ? track.size : 0); }
}

function validateUtf16(value: string): void {
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    if (code >= 0xd800 && code <= 0xdbff) {
      const next = value.charCodeAt(index + 1);
      if (!Number.isInteger(next) || next < 0xdc00 || next > 0xdfff) throw new TypeError("fast shared UTF-8 rejects an unpaired high surrogate");
      index += 1;
    } else if (code >= 0xdc00 && code <= 0xdfff) throw new TypeError("fast shared UTF-8 rejects an unpaired low surrogate");
  }
}

type LocalState = { ref: number; refGeneration: number; publishedGeneration: number; localDefIndex: number; visitEpoch: number; state: "unseen" | "visiting" | "emitted" };

type FastPatchValues = Partial<CanonicalDecoration> & { readonly wrap?: number; readonly align?: number; readonly gap?: number; readonly sequence?: PersistentSeq<BridgeLayoutChild>; readonly sequenceKind?: number; readonly gridSequence?: PersistentSeq<PackedGridCell> };
type FastPatch = { readonly base: BridgeViewNode; readonly kind: number; readonly mask: number; readonly values: FastPatchValues };

const sameArray = (left: readonly unknown[] | undefined, right: readonly unknown[] | undefined): boolean => left === right || (left !== undefined && right !== undefined && left.length === right.length && left.every((value, index) => value === right[index]));

export class FastSharedEncoder {
  readonly transport: FastSharedTransport;
  private readonly nodeStates = new WeakMap<object, LocalState>();
  private readonly sequenceStates = new WeakMap<object, LocalState>();
  private readonly touchedViews: BridgeViewNode[] = [];
  private readonly touchedSequences: PersistentSeqNode<unknown>[] = [];
  private nextRef = 1;
  private epoch = 0;
  private generationValue = 0;
  private definitions = 0;

  constructor(transport: FastSharedTransport) { this.transport = transport; }
  get generation(): number { return this.generationValue; }

  invalidateGeneration(): void {
    if (this.generationValue >= MAX_GENERATION - 1) throw new Error("fast shared generation exhausted");
    this.generationValue += 1;
    this.nextRef = 1;
  }

  render(view: View, hooks: FastHooks = {}): void {
    hooks.encodingStarted?.();
    let node: BridgeViewNode;
    try {
      node = nodeForBridge(view);
    } finally {
      hooks.encodingFinished?.();
    }
    const state = this.nodeState(node);
    if (state.publishedGeneration === this.generationValue) {
      hooks.nativeStarted?.();
      try {
        if (collectCounters) counters.fast_shared_exact_ref_calls += 1;
        this.transport.renderRef(this.generationValue, state.ref);
        return;
      } catch (error) {
        if (!isFastCacheMiss(error)) throw error;
        if (collectCounters) counters.fast_shared_cache_misses += 1;
        this.invalidateGeneration();
        hooks.cacheMiss?.();
        throw error;
      } finally {
        hooks.nativeFinished?.();
      }
    }
    hooks.encodingStarted?.();
    try {
      this.encode(node, false);
    } catch (error) {
      this.transport.releaseWritingPage();
      hooks.encodingFinished?.();
      throw error;
    }
    hooks.encodingFinished?.();
    try {
      hooks.nativeStarted?.();
      this.transport.commit(this.lastRoot);
      this.commitSuccessfulDefinitions();
    } catch (error) {
      if (!isFastCacheMiss(error)) throw error;
      if (collectCounters) counters.fast_shared_cache_misses += 1;
      this.invalidateGeneration();
      hooks.cacheMiss?.();
      throw error;
    } finally {
      hooks.nativeFinished?.();
    }
  }

  private lastRoot = 0;

  private encode(node: BridgeViewNode, cold: boolean): void {
    if (++this.epoch >= Number.MAX_SAFE_INTEGER) throw new Error("fast shared compile epoch exhausted");
    this.touchedViews.length = 0;
    this.touchedSequences.length = 0;
    this.definitions = 0;
    this.transport.begin(this.generationValue, cold ? 1 : 0);
    this.lastRoot = this.compileView(node, cold);
  }

  private commitSuccessfulDefinitions(): void {
    for (const node of this.touchedViews) { const state = this.nodeState(node); state.publishedGeneration = this.generationValue; }
    for (const sequence of this.touchedSequences) { const state = this.sequenceState(sequence); state.publishedGeneration = this.generationValue; }
    this.touchedViews.length = 0;
    this.touchedSequences.length = 0;
  }

  private nodeState(node: BridgeViewNode): LocalState {
    const existing = this.nodeStates.get(node);
    if (existing !== undefined) return existing;
    const state: LocalState = { ref: 0, refGeneration: -1, publishedGeneration: -1, localDefIndex: 0xffff_ffff, visitEpoch: 0, state: "unseen" };
    this.nodeStates.set(node, state);
    return state;
  }

  private sequenceState(node: object): LocalState {
    const existing = this.sequenceStates.get(node);
    if (existing !== undefined) return existing;
    const state: LocalState = { ref: 0, refGeneration: -1, publishedGeneration: -1, localDefIndex: 0xffff_ffff, visitEpoch: 0, state: "unseen" };
    this.sequenceStates.set(node, state);
    return state;
  }

  private ensureRef(state: LocalState): number {
    if (state.ref === 0 || state.refGeneration !== this.generationValue) {
      if (this.nextRef >= FAST_LOCAL_BIT - 256) throw new Error("fast shared refs exhausted");
      state.ref = this.nextRef++;
      state.refGeneration = this.generationValue;
    }
    return state.ref;
  }

  private compileView(node: BridgeViewNode, cold: boolean): number {
    const state = this.nodeState(node);
    if (!cold && state.publishedGeneration === this.generationValue) {
      if (collectCounters) counters.fast_shared_persistent_refs += 1;
      return this.transport.persistent(state.ref);
    }
    if (state.visitEpoch === this.epoch) {
      if (state.state === "visiting") throw new Error(`fast shared cyclic view dependency at NodeId ${node.id}`);
      if (collectCounters) counters.fast_shared_local_refs += 1;
      return this.transport.local(state.localDefIndex);
    }
    state.visitEpoch = this.epoch;
    state.state = "visiting";
    const meta = packedMeta(node);
    const patch = cold ? undefined : this.patchFor(node, meta.lineage?.base, meta.lineage?.kind);
    const nodeId = node.id;
    let op: number;
    let ref: number;
    if (patch !== undefined) {
      const base = this.compileView(patch.base, cold);
      const values = patch.kind === PACKED_V3.patchAxis
        ? { ...patch.values, sequence: patch.values.sequence!, sequenceKind: patch.values.sequenceKind!, sequenceRef: this.compileSequence(patch.values.sequence!.root, patch.values.sequenceKind!, cold) }
        : patch.kind === PACKED_V3.patchGrid
          ? { ...patch.values, gridSequenceRef: this.compileGridSequence(patch.values.gridSequence!.root, cold) }
          : patch.values;
      ref = this.ensureRef(state);
      op = this.emitPatch(ref, node, base, patch.kind, patch.mask, values);
    } else {
      const dependencies = this.compileDependencies(node, cold);
      ref = this.ensureRef(state);
      op = this.emitFull(ref, node, dependencies);
    }
    state.localDefIndex = op;
    state.state = "emitted";
    this.touchedViews.push(node);
    this.definitions += 1;
    return this.transport.local(op);
  }

  private compileDependencies(node: BridgeViewNode, cold: boolean): ViewDependencies {
    switch (node.kind) {
      case BRIDGE_VIEW_KIND.row:
      case BRIDGE_VIEW_KIND.column: {
        const sequence = packedMeta(node).sequence ?? PersistentSeq.from(node.children);
        return { sequenceRef: this.compileSequence(sequence.root, node.kind === BRIDGE_VIEW_KIND.row ? PACKED_V3.seqRow : PACKED_V3.seqColumn, cold) };
      }
      case BRIDGE_VIEW_KIND.hanging: return { prefix: this.compileView(node.prefix, cold), continuation: this.compileView(node.continuation, cold), body: this.compileView(node.body, cold) };
      case BRIDGE_VIEW_KIND.grid: return { grid: this.compileGridDependencies(node, cold) };
      case BRIDGE_VIEW_KIND.container:
      case BRIDGE_VIEW_KIND.contentMax:
      case BRIDGE_VIEW_KIND.clamp:
      case BRIDGE_VIEW_KIND.decorated: return { child: this.compileView(node.child, cold) };
      default: return {};
    }
  }

  private compileGridDependencies(node: Extract<BridgeViewNode, { kind: typeof BRIDGE_VIEW_KIND.grid }>, cold: boolean): GridDependencies {
    const cells = packedMeta(node).gridCells;
    if (cells === undefined) throw new Error("fast shared grid sequence is unavailable");
    return { columns: node.columns, rows: node.rows, cellSequenceRef: this.compileGridSequence(cells.root, cold), columnGap: node.columnGap, rowGap: node.rowGap };
  }

  private compileSequence(node: PersistentSeqNode<BridgeLayoutChild>, kind: number, cold: boolean): number {
    const state = this.sequenceState(node);
    if (!cold && state.publishedGeneration === this.generationValue) {
      if (collectCounters) counters.fast_shared_persistent_refs += 1;
      return this.transport.persistent(state.ref);
    }
    if (state.visitEpoch === this.epoch) {
      if (state.state === "visiting") throw new Error("fast shared cyclic sequence dependency");
      if (collectCounters) counters.fast_shared_local_refs += 1;
      return this.transport.local(state.localDefIndex);
    }
    state.visitEpoch = this.epoch;
    state.state = "visiting";
    const leafValues = node.kind === "leaf" ? node.items.map((item) => {
      const child = item as BridgeLayoutChild;
      return { child, view: this.compileView(child.child, cold) };
    }) : undefined;
    const branchValues = node.kind === "branch" ? node.children.map((child) => this.compileSequence(child, kind, cold)) : undefined;
    const payload = this.transport.payload((writer) => {
      if (node.kind === "leaf") {
        writer.push(kind); writer.push(leafValues!.length); writer.push(node.aggregate);
        for (const { child, view } of leafValues!) { writer.push(child.kind); writer.push("size" in child ? child.size : 0); writer.push("maxRows" in child ? child.maxRows : 0); writer.push(view); }
      } else {
        writer.push(kind); writer.push(node.height); writer.push(branchValues!.length); writer.push(node.aggregate);
        let previous = 0;
        for (let index = 0; index < branchValues!.length; index += 1) { previous += node.children[index]!.length; writer.push(previous); writer.push(branchValues![index]!); }
      }
    });
    const ref = this.ensureRef(state);
    const op = this.transport.emit(node.kind === "leaf" ? OP_DEF_SEQ_LEAF : OP_DEF_SEQ_BRANCH, ref, 0, 0, payload[0], payload[1]);
    state.localDefIndex = op; state.state = "emitted"; this.touchedSequences.push(node); return this.transport.local(op);
  }

  private compileGridSequence(node: PersistentSeqNode<PackedGridCell>, cold: boolean): number {
    const state = this.sequenceState(node);
    if (!cold && state.publishedGeneration === this.generationValue) {
      if (collectCounters) counters.fast_shared_persistent_refs += 1;
      return this.transport.persistent(state.ref);
    }
    if (state.visitEpoch === this.epoch) {
      if (state.state === "visiting") throw new Error("fast shared cyclic grid sequence dependency");
      if (collectCounters) counters.fast_shared_local_refs += 1;
      return this.transport.local(state.localDefIndex);
    }
    state.visitEpoch = this.epoch; state.state = "visiting";
    const leafValues = node.kind === "leaf" ? node.items.map((item) => ({ item, view: this.compileView(item.view, cold) })) : undefined;
    const branchValues = node.kind === "branch" ? node.children.map((child) => this.compileGridSequence(child, cold)) : undefined;
    const payload = this.transport.payload((writer) => {
      if (node.kind === "leaf") {
        writer.push(leafValues!.length); writer.push(node.aggregate);
        for (const { item, view } of leafValues!) { writer.push(item.row); writer.push(item.column); writer.push(item.columnSpan); writer.push(item.rowSpan); writer.push(item.horizontalAlign); writer.push(item.verticalAlign); writer.push(view); }
      } else {
        writer.push(node.height); writer.push(branchValues!.length); writer.push(node.aggregate);
        let previous = 0;
        for (let index = 0; index < branchValues!.length; index += 1) { previous += node.children[index]!.length; writer.push(previous); writer.push(branchValues![index]!); }
      }
    });
    const ref = this.ensureRef(state);
    const op = this.transport.emit(node.kind === "leaf" ? OP_DEF_GRID_LEAF : OP_DEF_GRID_BRANCH, ref, 0, 0, payload[0], payload[1]);
    state.localDefIndex = op; state.state = "emitted"; this.touchedSequences.push(node); return this.transport.local(op);
  }

  private emitFull(ref: number, node: BridgeViewNode, dependencies: ViewDependencies): number {
    const meta = packedMeta(node);
    switch (node.kind) {
      case BRIDGE_VIEW_KIND.text: {
        const payload = this.transport.payload((writer) => { writer.push(node.wrap); writer.push(node.align); writer.push(node.spans.length); for (const [index, span] of node.spans.entries()) { writer.string(span.text); writer.style(meta.textStyles?.[index]); } });
        return this.transport.emit(OP_DEF_TEXT, ref, 0, node.id, payload[0], payload[1]);
      }
      case BRIDGE_VIEW_KIND.diff: {
        const payload = this.transport.payload((writer) => this.writeDiff(writer, meta.diff!));
        return this.transport.emit(OP_DEF_DIFF, ref, 0, node.id, payload[0], payload[1]);
      }
      case BRIDGE_VIEW_KIND.spacer: return this.transport.emit(OP_DEF_SPACER, ref, 0, node.id, node.rows);
      case BRIDGE_VIEW_KIND.row:
      case BRIDGE_VIEW_KIND.column: return this.transport.emit(OP_DEF_AXIS, ref, 0, node.id, node.gap, dependencies.sequenceRef!);
      case BRIDGE_VIEW_KIND.hanging: return this.transport.emit(OP_DEF_HANGING, ref, 0, node.id, dependencies.prefix!, dependencies.continuation!, dependencies.body!);
      case BRIDGE_VIEW_KIND.grid: { const payload = this.transport.payload((writer) => this.writeGrid(writer, dependencies.grid!)); return this.transport.emit(OP_DEF_GRID, ref, 0, node.id, payload[0], payload[1]); }
      case BRIDGE_VIEW_KIND.container: return this.transport.emit(OP_DEF_CONTAINER, ref, 0, node.id, dependencies.child!);
      case BRIDGE_VIEW_KIND.clamp: { const payload = this.transport.payload((writer) => this.writeOverflow(writer, node.overflow, meta.overflowStyle)); return this.transport.emit(OP_DEF_CLAMP, ref, 0, node.id, dependencies.child!, payload[0], node.maxRows, payload[1]); }
      case BRIDGE_VIEW_KIND.contentMax: return this.transport.emit(OP_DEF_CONTENT_MAX, ref, 0, node.id, dependencies.child!, node.maxRows);
      case BRIDGE_VIEW_KIND.component: return this.transport.emit(OP_DEF_COMPONENT, ref, 0, node.id, node.handle % 0x1_0000_0000, Math.floor(node.handle / 0x1_0000_0000));
      case BRIDGE_VIEW_KIND.decorated: { const payload = this.transport.payload((writer) => this.writeDecoration(writer, meta.canonicalDecoration!)); return this.transport.emit(OP_DEF_DECORATED, ref, dependencies.child!, node.id, payload[0], payload[1]); }
      default: return assertNever(node);
    }
  }

  private emitPatch(ref: number, node: BridgeViewNode, base: number, kind: number, mask: number, values: FastPatchValues & { readonly sequenceRef?: number; readonly gridSequenceRef?: number }): number {
    if (kind === PACKED_V3.patchText) return this.transport.emit(OP_PATCH_TEXT, ref, base, node.id, mask, values.wrap ?? 0, values.align ?? 0);
    if (kind === PACKED_V3.patchAxis) return this.transport.emit(OP_PATCH_AXIS, ref, base, node.id, mask, values.sequenceRef!, values.gap ?? 0);
    if (kind === PACKED_V3.patchGrid) return this.transport.emit(OP_PATCH_GRID, ref, base, node.id, values.gridSequenceRef!);
    const payload = this.transport.payload((writer) => { writer.push(mask); if (mask & PACKED_V3.patchPadding) for (const value of values.padding!) writer.push(value); if (mask & PACKED_V3.patchWidth) writer.push(values.width === "fit" ? 1 : 2); if (mask & PACKED_V3.patchHeight) writer.push(values.height === "fit" ? 1 : 2); if (mask & PACKED_V3.patchMinWidth) writer.push(values.minWidth!); if (mask & PACKED_V3.patchMaxWidth) writer.push(values.maxWidth!); if (mask & PACKED_V3.patchMinHeight) writer.push(values.minHeight!); if (mask & PACKED_V3.patchMaxHeight) writer.push(values.maxHeight!); });
    return this.transport.emit(OP_PATCH_DECORATION, ref, base, node.id, payload[0], payload[1]);
  }

  private patchFor(node: BridgeViewNode, base: BridgeViewNode | undefined, lineageKind: string | undefined): FastPatch | undefined {
    if (base === undefined || lineageKind === undefined) return undefined;
    let candidate: BridgeViewNode | undefined = base;
    while (candidate !== undefined && this.nodeState(candidate).publishedGeneration !== this.generationValue) {
      const predecessor: PackedLineage | undefined = packedMeta(candidate).lineage;
      if (predecessor === undefined || predecessor.kind !== lineageKind) return undefined;
      candidate = predecessor.base;
    }
    if (candidate === undefined) return undefined;
    if (lineageKind === "text") {
      const target = node.kind === BRIDGE_VIEW_KIND.decorated ? node.child : node;
      const source = candidate.kind === BRIDGE_VIEW_KIND.decorated ? candidate.child : candidate;
      if (target.kind !== BRIDGE_VIEW_KIND.text || source.kind !== BRIDGE_VIEW_KIND.text || target.spans !== source.spans) return undefined;
      let mask = 0; if (target.wrap !== source.wrap) mask |= PACKED_V3.patchWrap; if (target.align !== source.align) mask |= PACKED_V3.patchAlign;
      return mask === 0 ? undefined : { base: candidate, kind: PACKED_V3.patchText, mask, values: { wrap: target.wrap, align: target.align } };
    }
    if (lineageKind === "decoration" && node.kind === BRIDGE_VIEW_KIND.decorated) {
      const target = packedMeta(node).canonicalDecoration!;
      const source = candidate.kind === BRIDGE_VIEW_KIND.decorated ? packedMeta(candidate).canonicalDecoration : undefined;
      let mask = 0; if (!sameArray(target.padding, source?.padding)) mask |= PACKED_V3.patchPadding; if (target.width !== source?.width) mask |= PACKED_V3.patchWidth; if (target.height !== source?.height) mask |= PACKED_V3.patchHeight; if (target.minWidth !== source?.minWidth) mask |= PACKED_V3.patchMinWidth; if (target.maxWidth !== source?.maxWidth) mask |= PACKED_V3.patchMaxWidth; if (target.minHeight !== source?.minHeight) mask |= PACKED_V3.patchMinHeight; if (target.maxHeight !== source?.maxHeight) mask |= PACKED_V3.patchMaxHeight;
      return mask === 0 ? undefined : { base: candidate, kind: PACKED_V3.patchDecoration, mask, values: target };
    }
    if (lineageKind === "grid" && node.kind === BRIDGE_VIEW_KIND.grid && candidate.kind === BRIDGE_VIEW_KIND.grid) {
      const target = packedMeta(node).gridCells; const source = packedMeta(candidate).gridCells;
      if (target === undefined || source === undefined || target.root === source.root) return undefined;
      return { base: candidate, kind: PACKED_V3.patchGrid, mask: PACKED_V3.patchGridCells, values: { gridSequence: target } };
    }
    if (lineageKind === "axis" && (node.kind === BRIDGE_VIEW_KIND.row || node.kind === BRIDGE_VIEW_KIND.column) && candidate.kind === node.kind) {
      const target = packedMeta(node).sequence; const source = packedMeta(candidate).sequence;
      if (target === undefined || source === undefined) return undefined;
      let mask = PACKED_V3.patchSequence; if (node.gap !== candidate.gap) mask |= PACKED_V3.patchGap;
      return { base: candidate, kind: PACKED_V3.patchAxis, mask, values: { gap: node.gap, sequence: target, sequenceKind: node.kind === BRIDGE_VIEW_KIND.row ? PACKED_V3.seqRow : PACKED_V3.seqColumn } };
    }
    return undefined;
  }

  private writeDiff(writer: FastMetaWriter, hunks: NonNullable<ReturnType<typeof packedMeta>["diff"]>): void {
    writer.push(hunks.length); for (const hunk of hunks) { writer.u64(hunk.oldRange[0]); writer.u64(hunk.oldCount[0]); writer.u64(hunk.newRange[0]); writer.u64(hunk.newCount[0]); writer.push(hunk.lines.length); for (const line of hunk.lines) { writer.push(line.kind); writer.string(line.text); writer.push(line.termination); if (line.oldLine !== undefined) writer.u64(line.oldLine[0]); if (line.newLine !== undefined) writer.u64(line.newLine[0]); } }
  }

  private writeGrid(writer: FastMetaWriter, grid: GridDependencies): void { writer.push(grid.columns.length); for (const track of grid.columns) writer.track(track); writer.push(grid.rows.length); for (const row of grid.rows) writer.track(row.track); writer.push(grid.cellSequenceRef); writer.push(grid.columnGap); writer.push(grid.rowGap); }

  private writeOverflow(writer: FastMetaWriter, overflow: BridgeOverflowIndicatorNode | undefined, style: CanonicalStyle | undefined): void { const value = overflow ?? { kind: 1 as const }; writer.push(value.kind); if (value.kind === 2) writer.style(style); if (value.kind === 3) { writer.string(value.prefix); writer.style(style); } }

  private writeDecoration(writer: FastMetaWriter, value: CanonicalDecoration): void {
    writer.push(value.flags); if (value.padding !== undefined) for (const item of value.padding) writer.push(item); if (value.background !== undefined) writer.color(value.background); if (value.foreground !== undefined) writer.color(value.foreground); if (value.border !== undefined) this.writeBorder(writer, value.border); writer.style(value.style); if (value.styleStates !== undefined) { writer.push(value.styleStates.length); for (const [key, state] of value.styleStates) { writer.string(key); writer.string(state); } } if (value.width !== undefined) writer.push(value.width === "fit" ? 1 : 2); if (value.height !== undefined) writer.push(value.height === "fit" ? 1 : 2); if (value.minWidth !== undefined) writer.push(value.minWidth); if (value.maxWidth !== undefined) writer.push(value.maxWidth); if (value.minHeight !== undefined) writer.push(value.minHeight); if (value.maxHeight !== undefined) writer.push(value.maxHeight);
  }

  private writeBorder(writer: FastMetaWriter, border: NonNullable<CanonicalDecoration["border"]>): void { writer.push(border.flags); if (border.glyphs !== undefined) for (const glyph of border.glyphs) writer.string(glyph); if (border.color !== undefined) writer.color(border.color); if (border.style !== undefined) writer.push(border.style); if (border.edges !== undefined) writer.push(border.edges); }
}

type ViewDependencies = { readonly sequenceRef?: number; readonly prefix?: number; readonly continuation?: number; readonly body?: number; readonly child?: number; readonly grid?: GridDependencies };
type GridDependencies = { readonly columns: readonly BridgeGridTrackNode[]; readonly rows: readonly { readonly track: BridgeGridTrackNode }[]; readonly cellSequenceRef: number; readonly columnGap: number; readonly rowGap: number };

function assertNever(value: never): never { throw new Error(`unsupported fast shared View kind ${(value as { readonly kind?: unknown }).kind ?? "unknown"}`); }
export function isFastUnsupported(error: unknown): boolean { return error instanceof FastSharedError && error.code === 6; }
export function isFastCacheMiss(error: unknown): boolean { return error instanceof FastSharedError ? error.code === FAST_CACHE_MISS : typeof error === "object" && error !== null && (((error as { readonly code?: unknown }).code === "ION_FAST_SHARED_1") || String((error as { readonly message?: unknown }).message ?? "").includes("ION_FAST_SHARED_1")); }
export function createFastSharedTransport(host: FastSharedHost): FastSharedTransport { const abi = host.tuiPerfFastSharedAbi?.(); if (abi === undefined) throw new Error("ION_FAST_SHARED_UNAVAILABLE"); return new FastSharedTransport(abi, host); }
export function createFastSharedEncoder(transport: FastSharedTransport): FastSharedEncoder { return new FastSharedEncoder(transport); }
export function replaceFastSharedAxisChild(view: View, index: number, child: View): View { return replaceAxisChildForPackedTransport(view, index, child); }
export function spliceFastSharedAxisChildren(view: View, index: number, removeCount: number, children: readonly View[]): View { return spliceAxisChildrenForPackedTransport(view, index, removeCount, children); }
export function replaceFastSharedGridCell(view: View, row: number, column: number, child: View): View { return replaceGridCellForPackedTransport(view, row, column, child); }

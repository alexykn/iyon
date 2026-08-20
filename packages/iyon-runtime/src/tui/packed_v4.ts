import {
  BRIDGE_VIEW_KIND,
  PACKED_V4,
  type BridgeGridCellNode,
  type BridgeGridRowNode,
  type BridgeGridTrackNode,
  type BridgeLayoutChild,
  type BridgeOverflowIndicatorNode,
  type BridgeViewNode,
} from "./ir.ts";
import { nodeForBridge, replaceAxisChildForPackedTransport, replaceGridCellForPackedTransport, spliceAxisChildrenForPackedTransport, type View } from "./values/view.ts";
import { packedMeta, packedSequenceMeta, type CanonicalDecoration, type CanonicalStyle, type PackedGridCell, type PackedLineage } from "./packed_v3_meta.ts";
import { PersistentSeq, type PersistentSeqNode } from "./persistent_seq.ts";

const HEADER_WORDS = 12;
const U32 = 0x1_0000_0000;
const MAX_U32 = U32 - 1;
const MAX_LOCAL = 0x7fff_ffff;
const UNASSIGNED = 0xffff_ffff;
const TEXT_PATCH_MASK = PACKED_V4.patchWrap | PACKED_V4.patchAlign;
const DECORATION_PATCH_MASK = PACKED_V4.patchPadding | PACKED_V4.patchWidth | PACKED_V4.patchHeight
  | PACKED_V4.patchMinWidth | PACKED_V4.patchMaxWidth | PACKED_V4.patchMinHeight | PACKED_V4.patchMaxHeight;
const AXIS_PATCH_MASK = PACKED_V4.patchGap | PACKED_V4.patchSequence;
const MAX_GENERATION = 0xffff_ffff;

// Packed refs and epochs are environment-wide, not encoder-instance-local.
// Multiple hosts/candidates share the environment cache, so restarting either
// counter in a new encoder would reintroduce a same-generation ABA collision.
let transportGeneration = 0;
let nextTransportRef = 1;
let nextTransactionEpoch = 0;

export type PackedV4Utf8Writer = "textencoder" | "buffer";
export type PackedV4StringDedupe = "content" | "identity" | "hybrid16" | "hybrid32" | "hybrid64" | "hybrid128";

export type PackedV4Transaction = {
  readonly words: Uint32Array;
  readonly bytes: Uint8Array;
  readonly generation: number;
  readonly definitionCount: number;
};
export type PackedV4Invoke = (words: Uint32Array, bytes: Uint8Array) => void;
export type PackedV4InvokeRef = (generation: number, packedRef: number) => void;
export type PackedV4Hooks = {
  readonly encodingStarted?: () => void;
  readonly encodingFinished?: () => void;
  readonly nativeStarted?: () => void;
  readonly nativeFinished?: () => void;
};

export type PackedV4Counters = {
  packed_v4_compile_objects_visited: number;
  packed_v4_full_view_defs: number;
  packed_v4_patch_view_defs: number;
  packed_v4_seq_leaf_defs: number;
  packed_v4_seq_branch_defs: number;
  packed_v4_persistent_refs: number;
  packed_v4_local_refs: number;
  packed_v4_words_used: number;
  packed_v4_bytes_used: number;
  packed_v4_word_buffer_grows: number;
  packed_v4_byte_buffer_grows: number;
  packed_v4_exact_ref_fast_hits: number;
  packed_v4_lineage_steps: number;
  packed_v4_patch_chains_collapsed: number;
  packed_v4_cache_resyncs: number;
  packed_v4_cold_retries: number;
  packed_v4_string_count: number;
  packed_v4_utf8_bytes: number;
  packed_v4_empty_refs: number;
  packed_v4_content_dedupe_lookups: number;
  packed_v4_content_dedupe_hits: number;
  packed_v4_utf8_writer_calls: number;
  packed_v4_offset_words: number;
  packed_v4_buffer_write_calls: number;
  packed_v4_byte_length_calls: number;
  packed_v4_textencoder_calls: number;
};

const counters: PackedV4Counters = {
  packed_v4_compile_objects_visited: 0,
  packed_v4_full_view_defs: 0,
  packed_v4_patch_view_defs: 0,
  packed_v4_seq_leaf_defs: 0,
  packed_v4_seq_branch_defs: 0,
  packed_v4_persistent_refs: 0,
  packed_v4_local_refs: 0,
  packed_v4_words_used: 0,
  packed_v4_bytes_used: 0,
  packed_v4_word_buffer_grows: 0,
  packed_v4_byte_buffer_grows: 0,
  packed_v4_exact_ref_fast_hits: 0,
  packed_v4_lineage_steps: 0,
  packed_v4_patch_chains_collapsed: 0,
  packed_v4_cache_resyncs: 0,
  packed_v4_cold_retries: 0,
  packed_v4_string_count: 0,
  packed_v4_utf8_bytes: 0,
  packed_v4_empty_refs: 0,
  packed_v4_content_dedupe_lookups: 0,
  packed_v4_content_dedupe_hits: 0,
  packed_v4_utf8_writer_calls: 0,
  packed_v4_offset_words: 0,
  packed_v4_buffer_write_calls: 0,
  packed_v4_byte_length_calls: 0,
  packed_v4_textencoder_calls: 0,
};

export function resetPackedV4Counters(): void {
  for (const key of Object.keys(counters) as (keyof PackedV4Counters)[]) counters[key] = 0;
}
export function packedV4Snapshot(): PackedV4Counters & Record<string, number> { return { ...counters }; }

class WordWriter {
  #buffer = new Uint32Array(256);
  #cursor = HEADER_WORDS;
  reset(): void { this.#cursor = HEADER_WORDS; }
  get position(): number { return this.#cursor; }
  get capacity(): number { return this.#buffer.length; }
  push(value: number): void {
    if (!Number.isInteger(value) || value < 0 || value >= U32) throw new RangeError("packed V4 word must be a uint32");
    this.ensure(1);
    this.#buffer[this.#cursor++] = value;
  }
  reserve(): number { const position = this.#cursor; this.push(0); return position; }
  patch(position: number, value: number): void {
    if (!Number.isInteger(value) || value < 0 || value >= U32 || position < 0 || position >= this.#cursor) throw new RangeError("invalid packed V4 word patch");
    this.#buffer[position] = value;
  }
  finish(): Uint32Array { return this.#buffer.subarray(0, this.#cursor); }
  private ensure(additional: number): void {
    const required = this.#cursor + additional;
    if (required <= this.#buffer.length) return;
    let size = this.#buffer.length;
    while (size < required) size *= 2;
    const next = new Uint32Array(size);
    next.set(this.#buffer);
    this.#buffer = next;
    counters.packed_v4_word_buffer_grows += 1;
  }
}

class ByteArena {
  #buffer: Uint8Array | Buffer;
  #cursor = 0;
  #indices = new Map<string, number>();
  #ends = [0];
  #encoder: TextEncoder | undefined;

  constructor(
    private readonly writer: PackedV4Utf8Writer,
    private readonly dedupe: PackedV4StringDedupe,
  ) {
    this.#buffer = writer === "buffer" ? Buffer.allocUnsafe(1024) : new Uint8Array(1024);
    this.#encoder = writer === "textencoder" ? new TextEncoder() : undefined;
  }

  reset(): void { this.#cursor = 0; this.#indices.clear(); this.#ends.length = 1; }
  get capacity(): number { return this.#buffer.length; }
  get usedBytes(): number { return this.#cursor; }
  get stringCount(): number { return this.#ends.length - 1; }
  get offsets(): readonly number[] { return this.#ends; }

  add(value: string): number {
    if (typeof value !== "string") throw new TypeError("packed V4 string must be a string");
    if (hasUnpairedSurrogate(value)) {
      throw new RangeError("packed V4 UTF-8 writers reject unpaired UTF-16 surrogates for direct-bridge parity");
    }
    if (value.length === 0) {
      counters.packed_v4_empty_refs += 1;
      return 0;
    }
    const useContentDedupe = this.shouldContentDedupe(value);
    if (useContentDedupe) {
      counters.packed_v4_content_dedupe_lookups += 1;
      const existing = this.#indices.get(value);
      if (existing !== undefined) {
        counters.packed_v4_content_dedupe_hits += 1;
        return existing;
      }
    }
    const start = this.#cursor;
    const conservativeMax = 3 * value.length;
    this.ensure(Number.isSafeInteger(conservativeMax) ? conservativeMax : 1);
    let written: number;
    if (this.writer === "buffer") {
      const available = this.#buffer.length - start;
      let expected: number | undefined;
      if (available < conservativeMax || !Number.isSafeInteger(conservativeMax)) {
        expected = Buffer.byteLength(value, "utf8");
        counters.packed_v4_byte_length_calls += 1;
        this.ensure(expected);
      }
      written = (this.#buffer as Buffer).write(value, start, this.#buffer.length - start, "utf8");
      counters.packed_v4_buffer_write_calls += 1;
      if (expected !== undefined && written !== expected) {
        throw new Error("packed V4 Buffer writer produced a truncated string");
      }
    } else {
      counters.packed_v4_textencoder_calls += 1;
      const encoder = this.#encoder!;
      let result = encoder.encodeInto(value, this.#buffer.subarray(start));
      while (result.read !== value.length) {
        // A partial encode only touched scratch bytes. Grow in place and retry
        // from the same cursor; no temporary encoded string is allocated.
        this.ensure(this.#buffer.length);
        result = encoder.encodeInto(value, this.#buffer.subarray(start));
      }
      written = result.written;
    }
    if (written > MAX_U32 - this.#cursor) {
      throw new RangeError("packed V4 UTF-8 arena exceeds u32 byte limit");
    }
    this.#cursor += written;
    if (this.#ends.length >= U32) throw new RangeError("packed V4 string table exceeds u32 entry limit");
    const ref = this.#ends.length;
    this.#ends.push(this.#cursor);
    if (useContentDedupe) this.#indices.set(value, ref);
    counters.packed_v4_string_count += 1;
    counters.packed_v4_utf8_bytes += written;
    return ref;
  }

  finish(): Uint8Array { return this.#buffer.subarray(0, this.#cursor); }

  private shouldContentDedupe(value: string): boolean {
    if (this.dedupe === "identity") return false;
    if (this.dedupe === "content") return true;
    const threshold = Number(this.dedupe.slice("hybrid".length));
    return value.length <= threshold;
  }

  private ensure(additional: number): void {
    if (!Number.isSafeInteger(additional) || additional < 0 || additional > MAX_U32 - this.#cursor) {
      throw new RangeError("packed V4 UTF-8 arena exceeds u32 byte limit");
    }
    const required = this.#cursor + additional;
    if (required <= this.#buffer.length) return;
    let size = this.#buffer.length;
    while (size < required) size *= 2;
    const next = this.writer === "buffer" ? Buffer.allocUnsafe(size) : new Uint8Array(size);
    next.set(this.#buffer.subarray(0, this.#cursor));
    this.#buffer = next;
    counters.packed_v4_byte_buffer_grows += 1;
  }
}

function hasUnpairedSurrogate(value: string): boolean {
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    if (code >= 0xd800 && code <= 0xdbff) {
      const next = value.charCodeAt(index + 1);
      if (next >= 0xdc00 && next <= 0xdfff) {
        index += 1;
        continue;
      }
      return true;
    }
    if (code >= 0xdc00 && code <= 0xdfff) return true;
  }
  return false;
}

function persistentRef(ref: number): number {
  if (!Number.isInteger(ref) || ref <= 0 || ref >= PACKED_V4.wireLocalBit) throw new RangeError("packed V4 persistent ref is out of range");
  return ref;
}
function localRef(index: number): number {
  if (!Number.isInteger(index) || index < 0 || index >= MAX_LOCAL) throw new RangeError("packed V4 local ref is out of range");
  return PACKED_V4.wireLocalBit + index;
}
function sameArray<T>(left: readonly T[] | undefined, right: readonly T[] | undefined): boolean {
  if (left === right) return true;
  if (left === undefined || right === undefined || left.length !== right.length) return false;
  return left.every((value, index) => value === right[index]);
}
export class PackedV4Encoder {
  #epoch = 0;
  #writer = new WordWriter();
  #bytes: ByteArena;
  readonly utf8Writer: PackedV4Utf8Writer;
  readonly stringDedupe: PackedV4StringDedupe;
  #touchedViews: BridgeViewNode[] = [];
  #touchedSequences: PersistentSeqNode<unknown>[] = [];
  #definitions = 0;

  constructor(utf8Writer: PackedV4Utf8Writer = "textencoder", stringDedupe: PackedV4StringDedupe = "content") {
    if (utf8Writer !== "textencoder" && utf8Writer !== "buffer") throw new RangeError("packed V4 UTF-8 writer is invalid");
    if (!["content", "identity", "hybrid16", "hybrid32", "hybrid64", "hybrid128"].includes(stringDedupe)) {
      throw new RangeError("packed V4 string dedupe policy is invalid");
    }
    this.utf8Writer = utf8Writer;
    this.stringDedupe = stringDedupe;
    this.#bytes = new ByteArena(utf8Writer, stringDedupe);
  }
  get generation(): number { return transportGeneration; }
  get wordScratchCapacity(): number { return this.#writer.capacity; }
  get byteScratchCapacity(): number { return this.#bytes.capacity; }

  invalidateGeneration(): void {
    if (transportGeneration >= MAX_GENERATION - 1) throw new Error("packed V4 transport generation exhausted");
    transportGeneration += 1;
    nextTransportRef = 1;
  }

  encodeRoots(roots: readonly BridgeViewNode[], cold = false): PackedV4Transaction {
    if (roots.length !== 1) throw new RangeError("packed V4 render currently accepts exactly one root");
    this.#writer.reset();
    this.#bytes.reset();
    if (nextTransactionEpoch >= Number.MAX_SAFE_INTEGER) throw new Error("packed V4 transaction epoch exhausted");
    this.#epoch = ++nextTransactionEpoch;
    this.#touchedViews = [];
    this.#touchedSequences = [];
    this.#definitions = 0;
    const wires = roots.map((root) => this.compileView(root, cold));
    const operationStart = this.#writer.position;
    this.#writer.push(roots.length === 1 ? PACKED_V4.opRender : PACKED_V4.opRenderForest);
    const operationLength = this.#writer.reserve();
    this.#writer.push(roots.length);
    for (const wire of wires) this.#writer.push(wire);
    this.#writer.patch(operationLength, this.#writer.position - operationStart);
    const recordsEndWord = this.#writer.position;
    const stringOffsetsStartWord = recordsEndWord;
    for (const offset of this.#bytes.offsets) this.#writer.push(offset);
    counters.packed_v4_offset_words += this.#bytes.offsets.length;
    const words = this.#writer.finish();
    const usedBytes = this.#bytes.usedBytes;
    if (usedBytes > MAX_U32) throw new RangeError("packed V4 UTF-8 arena exceeds u32 byte limit");
    words[0] = 0x49594f4e;
    words[1] = PACKED_V4.version;
    words[2] = 1;
    words[3] = transportGeneration;
    words[4] = (usedBytes === 0 ? 0 : PACKED_V4.hasUtf8)
      | (cold ? PACKED_V4.resetGeneration | PACKED_V4.coldClosure : 0);
    words[5] = words.length;
    words[6] = usedBytes;
    words[7] = roots.length;
    words[8] = this.#definitions;
    words[9] = recordsEndWord;
    words[10] = this.#bytes.stringCount;
    words[11] = stringOffsetsStartWord;
    counters.packed_v4_words_used += words.length;
    counters.packed_v4_bytes_used += usedBytes;
    return {
      words,
      bytes: this.#bytes.finish(),
      generation: transportGeneration,
      definitionCount: this.#definitions,
    };
  }

  commitSuccessfulDefinitions(): void {
    for (const node of this.#touchedViews) packedMeta(node).publishedGeneration = transportGeneration;
    for (const node of this.#touchedSequences) packedSequenceMeta(node).publishedGeneration = transportGeneration;
    this.#touchedViews = [];
    this.#touchedSequences = [];
  }

  render(root: View, invoke: PackedV4Invoke, invokeRef: PackedV4InvokeRef, hooks: PackedV4Hooks = {}): void {
    const node = nodeForBridge(root);
    const meta = packedMeta(node);
    let resynchronize = false;
    if (meta.publishedGeneration === transportGeneration) {
      const ref = this.ensureViewRef(meta);
      counters.packed_v4_exact_ref_fast_hits += 1;
      hooks.nativeStarted?.();
      try {
        invokeRef(transportGeneration, ref);
        return;
      } catch (error) {
        if (!isPackedV4CacheMiss(error)) throw error;
        counters.packed_v4_cache_resyncs += 1;
        this.#touchedViews = [];
        this.#touchedSequences = [];
        this.invalidateGeneration();
        resynchronize = true;
      } finally {
        hooks.nativeFinished?.();
      }
    }
    if (!resynchronize && nextTransportRef >= PACKED_V4.wireLocalBit - 256) {
      counters.packed_v4_cache_resyncs += 1;
      this.invalidateGeneration();
      resynchronize = true;
    }
    if (!resynchronize) {
      hooks.encodingStarted?.();
      let transaction: PackedV4Transaction;
      try {
        transaction = this.encodeRoots([node]);
      } finally {
        hooks.encodingFinished?.();
      }
      try {
        hooks.nativeStarted?.();
        invokeTransaction(transaction, invoke);
        this.commitSuccessfulDefinitions();
        return;
      } catch (error) {
        this.#touchedViews = [];
        this.#touchedSequences = [];
        if (!isPackedV4CacheMiss(error)) throw error;
        counters.packed_v4_cache_resyncs += 1;
        this.invalidateGeneration();
      } finally {
        hooks.nativeFinished?.();
      }
    }
    counters.packed_v4_cold_retries += 1;
    hooks.encodingStarted?.();
    let coldTransaction: PackedV4Transaction;
    try {
      coldTransaction = this.encodeRoots([node], true);
    } finally {
      hooks.encodingFinished?.();
    }
    hooks.nativeStarted?.();
    try {
      invokeTransaction(coldTransaction, invoke);
      this.commitSuccessfulDefinitions();
    } finally {
      hooks.nativeFinished?.();
    }
  }

  private ensureViewRef(meta: ReturnType<typeof packedMeta>): number {
    if (meta.refGeneration === transportGeneration) return persistentRef(meta.ref);
    if (nextTransportRef >= PACKED_V4.wireLocalBit - 256) throw new Error("packed V4 persistent ref generation exhausted");
    meta.ref = nextTransportRef++;
    meta.refGeneration = transportGeneration;
    meta.publishedGeneration = 0;
    return meta.ref;
  }

  private ensureSequenceRef(meta: ReturnType<typeof packedSequenceMeta>): number {
    if (meta.refGeneration === transportGeneration) return persistentRef(meta.ref);
    if (nextTransportRef >= PACKED_V4.wireLocalBit - 256) throw new Error("packed V4 persistent ref generation exhausted");
    meta.ref = nextTransportRef++;
    meta.refGeneration = transportGeneration;
    meta.publishedGeneration = 0;
    return meta.ref;
  }

  private compileView(node: BridgeViewNode, cold: boolean): number {
    counters.packed_v4_compile_objects_visited += 1;
    const meta = packedMeta(node);
    if (!cold && meta.publishedGeneration === transportGeneration) {
      counters.packed_v4_persistent_refs += 1;
      return this.ensureViewRef(meta);
    }
    if (meta.visitEpoch === this.#epoch) {
      if (meta.state === "visiting") throw new Error(`packed V4 cyclic semantic dependency at NodeId ${node.id}`);
      counters.packed_v4_local_refs += 1;
      return localRef(meta.localDefIndex);
    }
    meta.visitEpoch = this.#epoch;
    meta.state = "visiting";
    const patch = cold ? undefined : this.patchFor(node, meta.lineage?.base, meta.lineage?.kind);
    if (patch !== undefined) {
      const base = this.compileView(patch.base, cold);
      const values = patch.kind === PACKED_V4.patchAxis
        ? { ...patch.values, sequenceRef: this.compileSequence(patch.values.sequence!.root, patch.values.sequenceKind!, cold) }
        : patch.kind === PACKED_V4.patchGrid
          ? { ...patch.values, gridSequenceRef: this.compileGridSequence(patch.values.gridSequence!.root, cold) }
          : patch.values;
      const ref = this.ensureViewRef(meta);
      meta.localDefIndex = this.#definitions++;
      this.emitPatch(ref, node, base, patch.kind, patch.mask, values);
      meta.state = "emitted";
      this.#touchedViews.push(node);
      counters.packed_v4_patch_view_defs += 1;
      return localRef(meta.localDefIndex);
    }
    const ref = this.ensureViewRef(meta);
    const dependencies = this.compileDependencies(node, cold);
    meta.localDefIndex = this.#definitions++;
    this.emitFull(ref, node, dependencies);
    meta.state = "emitted";
    this.#touchedViews.push(node);
    counters.packed_v4_full_view_defs += 1;
    return localRef(meta.localDefIndex);
  }

  private compileDependencies(node: BridgeViewNode, cold: boolean): ViewDependencies {
    switch (node.kind) {
      case BRIDGE_VIEW_KIND.row:
      case BRIDGE_VIEW_KIND.column: {
        const sequence = packedMeta(node).sequence ?? PersistentSeq.from(node.children);
        const sequenceRef = this.compileSequence(sequence.root, node.kind === BRIDGE_VIEW_KIND.row ? PACKED_V4.seqRow : PACKED_V4.seqColumn, cold);
        return { sequenceRef };
      }
      case BRIDGE_VIEW_KIND.hanging:
        return { prefix: this.compileView(node.prefix, cold), continuation: this.compileView(node.continuation, cold), body: this.compileView(node.body, cold) };
      case BRIDGE_VIEW_KIND.grid:
        return { grid: this.compileGridDependencies(node, cold) };
      case BRIDGE_VIEW_KIND.container:
      case BRIDGE_VIEW_KIND.contentMax:
        return { child: this.compileView(node.child, cold) };
      case BRIDGE_VIEW_KIND.clamp:
        return { child: this.compileView(node.child, cold) };
      case BRIDGE_VIEW_KIND.decorated:
        return { child: this.compileView(node.child, cold) };
      default:
        return {};
    }
  }

  private compileGridDependencies(node: Extract<BridgeViewNode, { kind: typeof BRIDGE_VIEW_KIND.grid }>, cold: boolean): GridDependencies {
    const cells = packedMeta(node).gridCells;
    if (cells === undefined) throw new Error("packed V4 grid sequence is unavailable");
    return {
      columns: node.columns,
      rows: node.rows.map((row) => ({ track: row.track })),
      cellSequenceRef: this.compileGridSequence(cells.root, cold),
      columnGap: node.columnGap,
      rowGap: node.rowGap,
    };
  }

  private compileSequence(node: PersistentSeqNode<BridgeLayoutChild>, kind: number, cold: boolean): number {
    const meta = packedSequenceMeta(node);
    if (!cold && meta.publishedGeneration === transportGeneration) {
      counters.packed_v4_persistent_refs += 1;
      return this.ensureSequenceRef(meta);
    }
    if (meta.visitEpoch === this.#epoch) {
      if (meta.state === "visiting") throw new Error("packed V4 cyclic sequence dependency");
      counters.packed_v4_local_refs += 1;
      return localRef(meta.localDefIndex);
    }
    meta.visitEpoch = this.#epoch;
    meta.state = "visiting";
    const ref = this.ensureSequenceRef(meta);
    if (node.kind === "leaf") {
      const values = node.items.map((item) => ({ item, view: this.compileView(item.child, cold) }));
      meta.localDefIndex = this.#definitions++;
      this.emitSequenceLeaf(ref, kind, node.aggregate, values);
      counters.packed_v4_seq_leaf_defs += 1;
    } else {
      let cumulative = 0;
      const children = node.children.map((child) => {
        cumulative += child.length;
        return { size: cumulative, ref: this.compileSequence(child, kind, cold) };
      });
      meta.localDefIndex = this.#definitions++;
      this.emitSequenceBranch(ref, kind, node.height, node.aggregate, children);
      counters.packed_v4_seq_branch_defs += 1;
    }
    meta.state = "emitted";
    this.#touchedSequences.push(node);
    return localRef(meta.localDefIndex);
  }

  private emitFull(ref: number, node: BridgeViewNode, dependencies: ViewDependencies): void {
    const meta = packedMeta(node);
    this.record(PACKED_V4.defViewFull, ref, () => {
      this.writeMetaNodeId(meta);
      this.#writer.push(meta.recipe.kind);
      switch (node.kind) {
        case BRIDGE_VIEW_KIND.text: {
          const styles = meta.textStyles;
          this.#writer.push(node.wrap); this.#writer.push(node.align); this.#writer.push(node.spans.length);
          for (const [index, span] of node.spans.entries()) {
            this.writeString(span.text);
            this.writeStyle(styles?.[index]);
          }
          break;
        }
        case BRIDGE_VIEW_KIND.diff: this.writeDiff(meta.diff!); break;
        case BRIDGE_VIEW_KIND.spacer: this.#writer.push(node.rows); break;
        case BRIDGE_VIEW_KIND.row:
        case BRIDGE_VIEW_KIND.column: this.#writer.push(node.gap); this.#writer.push(dependencies.sequenceRef!); break;
        case BRIDGE_VIEW_KIND.hanging: this.#writer.push(dependencies.prefix!); this.#writer.push(dependencies.continuation!); this.#writer.push(dependencies.body!); break;
        case BRIDGE_VIEW_KIND.grid: this.writeGrid(dependencies.grid!); break;
        case BRIDGE_VIEW_KIND.container: this.#writer.push(dependencies.child!); break;
        case BRIDGE_VIEW_KIND.clamp: this.#writer.push(node.maxRows!); this.writeOverflow(node.overflow, meta.overflowStyle); this.#writer.push(dependencies.child!); break;
        case BRIDGE_VIEW_KIND.contentMax: this.#writer.push(node.maxRows); this.#writer.push(dependencies.child!); break;
        case BRIDGE_VIEW_KIND.component: {
          const handle = meta.recipe.componentHandle;
          if (handle === undefined) throw new TypeError("packed V4 component handle is missing");
          this.#writer.push(handle[0]); this.#writer.push(handle[1]);
          break;
        }
        case BRIDGE_VIEW_KIND.decorated: this.#writer.push(dependencies.child!); this.writeDecoration(packedMeta(node).canonicalDecoration!); break;
        default: assertNever(node);
      }
    });
  }

  private emitPatch(ref: number, node: BridgeViewNode, base: number, kind: number, mask: number, values: PatchValues): void {
    const meta = packedMeta(node);
    this.record(PACKED_V4.patchView, ref, () => {
      this.writeMetaNodeId(meta);
      this.#writer.push(base);
      this.#writer.push(kind);
      this.#writer.push(mask);
      if (kind === PACKED_V4.patchText) {
        if (mask & PACKED_V4.patchWrap) this.#writer.push(values.wrap!);
        if (mask & PACKED_V4.patchAlign) this.#writer.push(values.align!);
      } else if (kind === PACKED_V4.patchAxis) {
        if (mask & PACKED_V4.patchGap) this.#writer.push(values.gap!);
        if (mask & PACKED_V4.patchSequence) this.#writer.push(values.sequenceRef!);
      } else if (kind === PACKED_V4.patchGrid) {
        this.#writer.push(values.gridSequenceRef!);
      } else {
        if (mask & PACKED_V4.patchPadding) for (const value of values.padding!) this.#writer.push(value);
        if (mask & PACKED_V4.patchWidth) this.#writer.push(values.width === "fit" ? 1 : 2);
        if (mask & PACKED_V4.patchHeight) this.#writer.push(values.height === "fit" ? 1 : 2);
        if (mask & PACKED_V4.patchMinWidth) this.#writer.push(values.minWidth!);
        if (mask & PACKED_V4.patchMaxWidth) this.#writer.push(values.maxWidth!);
        if (mask & PACKED_V4.patchMinHeight) this.#writer.push(values.minHeight!);
        if (mask & PACKED_V4.patchMaxHeight) this.#writer.push(values.maxHeight!);
      }
    });
  }

  private compileGridSequence(node: PersistentSeqNode<PackedGridCell>, cold: boolean): number {
    const meta = packedSequenceMeta(node);
    if (!cold && meta.publishedGeneration === transportGeneration) {
      counters.packed_v4_persistent_refs += 1;
      return this.ensureSequenceRef(meta);
    }
    if (meta.visitEpoch === this.#epoch) {
      if (meta.state === "visiting") throw new Error("packed V4 cyclic grid sequence dependency");
      counters.packed_v4_local_refs += 1;
      return localRef(meta.localDefIndex);
    }
    meta.visitEpoch = this.#epoch;
    meta.state = "visiting";
    const ref = this.ensureSequenceRef(meta);
    if (node.kind === "leaf") {
      const values = node.items.map((item) => ({ item, view: this.compileView(item.view, cold) }));
      meta.localDefIndex = this.#definitions++;
      this.emitGridSequenceLeaf(ref, node.aggregate, values);
      counters.packed_v4_seq_leaf_defs += 1;
    } else {
      let cumulative = 0;
      const children = node.children.map((child) => {
        cumulative += child.length;
        return { size: cumulative, ref: this.compileGridSequence(child, cold) };
      });
      meta.localDefIndex = this.#definitions++;
      this.emitGridSequenceBranch(ref, node.height, node.aggregate, children);
      counters.packed_v4_seq_branch_defs += 1;
    }
    meta.state = "emitted";
    this.#touchedSequences.push(node);
    return localRef(meta.localDefIndex);
  }

  private emitSequenceLeaf(ref: number, kind: number, aggregate: number, values: readonly { item: BridgeLayoutChild; view: number }[]): void {
    this.record(PACKED_V4.defSeqLeaf, ref, () => {
      this.#writer.push(kind); this.#writer.push(values.length); this.#writer.push(aggregate);
      for (const { item, view } of values) {
        this.#writer.push(item.kind);
        this.#writer.push("size" in item ? item.size : 0);
        this.#writer.push("maxRows" in item ? item.maxRows : 0);
        this.#writer.push(view);
      }
    });
  }

  private emitSequenceBranch(ref: number, kind: number, height: number, aggregate: number, children: readonly { size: number; ref: number }[]): void {
    this.record(PACKED_V4.defSeqBranch, ref, () => {
      this.#writer.push(kind); this.#writer.push(height); this.#writer.push(children.length); this.#writer.push(aggregate);
      for (const child of children) { this.#writer.push(child.size); this.#writer.push(child.ref); }
    });
  }

  private emitGridSequenceLeaf(ref: number, aggregate: number, values: readonly { item: PackedGridCell; view: number }[]): void {
    this.record(PACKED_V4.defGridCellLeaf, ref, () => {
      this.#writer.push(values.length); this.#writer.push(aggregate);
      for (const { item, view } of values) {
        this.#writer.push(item.row); this.#writer.push(item.column);
        this.#writer.push(item.columnSpan); this.#writer.push(item.rowSpan);
        this.#writer.push(item.horizontalAlign); this.#writer.push(item.verticalAlign); this.#writer.push(view);
      }
    });
  }

  private emitGridSequenceBranch(ref: number, height: number, aggregate: number, children: readonly { size: number; ref: number }[]): void {
    this.record(PACKED_V4.defGridCellBranch, ref, () => {
      this.#writer.push(height); this.#writer.push(children.length); this.#writer.push(aggregate);
      for (const child of children) { this.#writer.push(child.size); this.#writer.push(child.ref); }
    });
  }

  private patchFor(node: BridgeViewNode, base: BridgeViewNode | undefined, lineageKind: string | undefined): Patch | undefined {
    if (base === undefined || lineageKind === undefined) return undefined;
    let candidate: BridgeViewNode | undefined = base;
    let lineageSteps = 0;
    while (candidate !== undefined && packedMeta(candidate).publishedGeneration !== transportGeneration) {
      const predecessor: PackedLineage | undefined = packedMeta(candidate).lineage;
      if (predecessor === undefined || predecessor.kind !== lineageKind) return undefined;
      candidate = predecessor.base;
      lineageSteps += 1;
    }
    if (candidate === undefined) return undefined;
    const publishedBase = candidate;
    counters.packed_v4_lineage_steps += lineageSteps + 1;
    if (lineageSteps > 0) counters.packed_v4_patch_chains_collapsed += 1;
    if (lineageKind === "text") {
      const target = node.kind === BRIDGE_VIEW_KIND.decorated ? node.child : node;
      const source = publishedBase.kind === BRIDGE_VIEW_KIND.decorated ? publishedBase.child : publishedBase;
      if (target.kind !== BRIDGE_VIEW_KIND.text || source.kind !== BRIDGE_VIEW_KIND.text || target.spans !== source.spans) return undefined;
      let mask = 0;
      if (target.wrap !== source.wrap) mask |= PACKED_V4.patchWrap;
      if (target.align !== source.align) mask |= PACKED_V4.patchAlign;
      if (mask === 0 || mask > TEXT_PATCH_MASK) return undefined;
      return { base: publishedBase, kind: PACKED_V4.patchText, mask, values: { wrap: target.wrap, align: target.align } };
    }
    if (lineageKind === "decoration" && node.kind === BRIDGE_VIEW_KIND.decorated) {
      const target = packedMeta(node).canonicalDecoration!;
      const source = publishedBase.kind === BRIDGE_VIEW_KIND.decorated ? packedMeta(publishedBase).canonicalDecoration : undefined;
      let mask = 0;
      if (!sameArray(target.padding, source?.padding)) mask |= PACKED_V4.patchPadding;
      if (target.width !== source?.width) mask |= PACKED_V4.patchWidth;
      if (target.height !== source?.height) mask |= PACKED_V4.patchHeight;
      if (target.minWidth !== source?.minWidth) mask |= PACKED_V4.patchMinWidth;
      if (target.maxWidth !== source?.maxWidth) mask |= PACKED_V4.patchMaxWidth;
      if (target.minHeight !== source?.minHeight) mask |= PACKED_V4.patchMinHeight;
      if (target.maxHeight !== source?.maxHeight) mask |= PACKED_V4.patchMaxHeight;
      if (mask === 0 || mask & ~DECORATION_PATCH_MASK) return undefined;
      return { base: publishedBase, kind: PACKED_V4.patchDecoration, mask, values: target };
    }
    if (lineageKind === "grid"
      && node.kind === BRIDGE_VIEW_KIND.grid
      && publishedBase.kind === BRIDGE_VIEW_KIND.grid) {
      const targetSequence = packedMeta(node).gridCells;
      const baseSequence = packedMeta(publishedBase).gridCells;
      if (targetSequence === undefined || baseSequence === undefined || targetSequence.root === baseSequence.root) return undefined;
      return {
        base: publishedBase,
        kind: PACKED_V4.patchGrid,
        mask: PACKED_V4.patchGridCells,
        values: { gridSequence: targetSequence },
      };
    }
    if (lineageKind === "axis"
      && (node.kind === BRIDGE_VIEW_KIND.row || node.kind === BRIDGE_VIEW_KIND.column)
      && publishedBase.kind === node.kind) {
      const targetSequence = packedMeta(node).sequence;
      const baseSequence = packedMeta(publishedBase).sequence;
      if (targetSequence === undefined || baseSequence === undefined) return undefined;
      let mask = PACKED_V4.patchSequence;
      if (node.gap !== publishedBase.gap) mask |= PACKED_V4.patchGap;
      if (mask & ~AXIS_PATCH_MASK) return undefined;
      return {
        base: publishedBase,
        kind: PACKED_V4.patchAxis,
        mask,
        values: {
          gap: node.gap,
          sequence: targetSequence,
          sequenceKind: node.kind === BRIDGE_VIEW_KIND.row ? PACKED_V4.seqRow : PACKED_V4.seqColumn,
        },
      };
    }
    return undefined;
  }

  private record(tag: number, ref: number, body: (lengthPosition: number) => void): void {
    const start = this.#writer.position;
    this.#writer.push(tag);
    const length = this.#writer.reserve();
    this.#writer.push(ref);
    body(length);
    this.#writer.patch(length, this.#writer.position - start);
  }

  private writeMetaNodeId(meta: ReturnType<typeof packedMeta>): void {
    this.#writer.push(meta.recipe.nodeIdLow);
    this.#writer.push(meta.recipe.nodeIdHigh);
  }
  private writeString(value: string): void {
    this.#writer.push(this.#bytes.add(value));
  }
  private writeStyle(style: CanonicalStyle | undefined): void {
    if (style === undefined) { this.#writer.push(0); this.#writer.push(0); this.#writer.push(0); return; }
    this.#writer.push(style.flags);
    if (style.flags & 1) this.writeString(style.theme!);
    if (style.flags & 2) this.writeColor(style.foreground!);
    if (style.flags & 4) this.writeColor(style.background!);
    this.#writer.push(style.attributePresent); this.#writer.push(style.attributeTrue);
  }
  private writeColor(color: CanonicalStyle["foreground"]): void {
    if (color === undefined) throw new TypeError("packed V4 canonical color is missing");
    if (color.kind === "string") { this.#writer.push(1); this.writeString(color.value); return; }
    this.#writer.push(2); this.#writer.push(color.value);
  }
  private writeDiff(hunks: NonNullable<ReturnType<typeof packedMeta>["diff"]>): void {
    this.#writer.push(hunks.length);
    for (const hunk of hunks) {
      this.#writer.push(hunk.oldRange[0]); this.#writer.push(hunk.oldRange[1]);
      this.#writer.push(hunk.oldCount[0]); this.#writer.push(hunk.oldCount[1]);
      this.#writer.push(hunk.newRange[0]); this.#writer.push(hunk.newRange[1]);
      this.#writer.push(hunk.newCount[0]); this.#writer.push(hunk.newCount[1]);
      this.#writer.push(hunk.lines.length);
      for (const line of hunk.lines) {
        this.#writer.push(line.kind);
        this.writeString(line.text);
        this.#writer.push(line.termination);
        if (line.oldLine !== undefined) { this.#writer.push(line.oldLine[0]); this.#writer.push(line.oldLine[1]); }
        if (line.newLine !== undefined) { this.#writer.push(line.newLine[0]); this.#writer.push(line.newLine[1]); }
      }
    }
  }
  private writeGrid(grid: GridDependencies): void {
    this.#writer.push(grid.columns.length); for (const track of grid.columns) this.writeTrack(track);
    this.#writer.push(grid.rows.length); for (const row of grid.rows) this.writeTrack(row.track);
    this.#writer.push(grid.cellSequenceRef);
    this.#writer.push(grid.columnGap); this.#writer.push(grid.rowGap);
  }
  private writeTrack(track: BridgeGridTrackNode): void { this.#writer.push(track.kind); this.#writer.push("max" in track ? track.max : "size" in track ? track.size : 0); }
  private writeOverflow(overflow: BridgeOverflowIndicatorNode | undefined, canonical?: CanonicalStyle): void {
    const value = overflow ?? { kind: 1 };
    this.#writer.push(value.kind);
    if (value.kind === 2) this.writeStyle(canonical);
    if (value.kind === 3) { this.writeString(value.prefix); this.writeStyle(canonical); }
  }
  private writeDecoration(value: CanonicalDecoration): void {
    this.#writer.push(value.flags);
    if (value.padding !== undefined) for (const item of value.padding) this.#writer.push(item);
    if (value.background !== undefined) this.writeColor(value.background);
    if (value.foreground !== undefined) this.writeColor(value.foreground);
    if (value.border !== undefined) this.writeBorder(value.border);
    this.writeStyle(value.style);
    if (value.styleStates !== undefined) { this.#writer.push(value.styleStates.length); for (const [key, state] of value.styleStates) { this.writeString(key); this.writeString(state); } }
    if (value.width !== undefined) this.#writer.push(value.width === "fit" ? 1 : 2);
    if (value.height !== undefined) this.#writer.push(value.height === "fit" ? 1 : 2);
    if (value.minWidth !== undefined) this.#writer.push(value.minWidth);
    if (value.maxWidth !== undefined) this.#writer.push(value.maxWidth);
    if (value.minHeight !== undefined) this.#writer.push(value.minHeight);
    if (value.maxHeight !== undefined) this.#writer.push(value.maxHeight);
  }
  private writeBorder(border: NonNullable<CanonicalDecoration["border"]>): void {
    this.#writer.push(border.flags);
    if (border.glyphs !== undefined) for (const glyph of border.glyphs) this.writeString(glyph);
    if (border.color !== undefined) this.writeColor(border.color);
    if (border.style !== undefined) this.#writer.push(border.style);
    if (border.edges !== undefined) this.#writer.push(border.edges);
  }
}

type ViewDependencies = { readonly sequenceRef?: number; readonly prefix?: number; readonly continuation?: number; readonly body?: number; readonly child?: number; readonly grid?: GridDependencies };
type GridDependencies = { readonly columns: readonly BridgeGridTrackNode[]; readonly rows: readonly { readonly track: BridgeGridTrackNode }[]; readonly cellSequenceRef: number; readonly columnGap: number; readonly rowGap: number };
type PatchValues = Partial<CanonicalDecoration> & {
  readonly wrap?: number;
  readonly align?: number;
  readonly gap?: number;
  readonly sequence?: PersistentSeq<BridgeLayoutChild>;
  readonly sequenceKind?: number;
  readonly sequenceRef?: number;
  readonly gridSequence?: PersistentSeq<PackedGridCell>;
  readonly gridSequenceRef?: number;
};
type Patch = { readonly base: BridgeViewNode; readonly kind: number; readonly mask: number; readonly values: PatchValues };

function invokeTransaction(transaction: PackedV4Transaction, invoke: PackedV4Invoke): void {
  invoke(transaction.words, transaction.bytes);
}
export function isPackedV4CacheMiss(error: unknown): boolean { return typeof error === "object" && error !== null && (((error as { readonly code?: unknown }).code === "ION_PACKED_CACHE_MISS") || String((error as { readonly message?: unknown }).message ?? "").includes("ION_PACKED_CACHE_MISS")); }
export function createPackedV4Encoder(utf8Writer: PackedV4Utf8Writer = "textencoder", stringDedupe: PackedV4StringDedupe = "content"): PackedV4Encoder {
  return new PackedV4Encoder(utf8Writer, stringDedupe);
}
export function replacePackedAxisChild(view: View, index: number, child: View): View { return replaceAxisChildForPackedTransport(view, index, child); }
export function splicePackedAxisChildren(view: View, index: number, removeCount: number, children: readonly View[]): View { return spliceAxisChildrenForPackedTransport(view, index, removeCount, children); }
export function replacePackedGridCell(view: View, row: number, column: number, child: View): View { return replaceGridCellForPackedTransport(view, row, column, child); }
export function renderPackedV4View(encoder: PackedV4Encoder, view: View, invoke: PackedV4Invoke, invokeRef: PackedV4InvokeRef, hooks?: PackedV4Hooks): void {
  encoder.render(view, invoke, invokeRef, hooks);
}
function assertNever(value: never): never { throw new Error(`unsupported packed V4 View kind ${(value as { readonly kind?: unknown }).kind ?? "unknown"}`); }

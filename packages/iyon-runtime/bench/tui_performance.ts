import { native } from "../src/native.ts";
import {
  BRIDGE_VIEW_KIND,
  type BridgeViewNode,
} from "../src/tui/ir.ts";
import { View, nodeForBridge, nodeForDirectBridge } from "../src/tui/values/view.ts";
import type { NativeHandleId } from "../src/tui/types.ts";
import { TextSpan } from "../src/tui/values/text.ts";
import { Style } from "../src/tui/values/style.ts";
import { DiffHunk, DiffLine, DiffRange, DiffRenderer } from "../src/tui/values/diff.ts";
import {
  createPackedViewEncoder,
  packedEncoderSnapshot,
  renderPackedView,
  resetPackedEncoderCounters,
  type PackedViewEncoder,
} from "../src/tui/packed.ts";
import {
  createPackedV3Encoder,
  packedV3Snapshot,
  renderPackedV3View,
  replacePackedAxisChild,
  replacePackedGridCell,
  resetPackedV3Counters,
  splicePackedAxisChildren,
  type PackedV3Encoder,
} from "../src/tui/packed_v3.ts";
import {
  createPackedV4Encoder,
  packedV4Snapshot,
  renderPackedV4View,
  replacePackedAxisChild as replacePackedV4AxisChild,
  replacePackedGridCell as replacePackedV4GridCell,
  resetPackedV4Counters,
  splicePackedAxisChildren as splicePackedV4AxisChildren,
  type PackedV4Encoder,
  type PackedV4StringDedupe,
  type PackedV4Utf8Writer,
} from "../src/tui/packed_v4.ts";

type Pattern = "COLD" | "FIRST_USE" | "IDENTICAL_IDENTITY" | "SHARED_PATH" | "REBUILT_EQUIVALENT" | "LARGE_SHARED_SUBTREE_CUTOFF" | "SHARED_DEEP" | "WIDE_PARENT_ONE_EDIT" | "WIDE_PARENT_INSERT" | "WIDE_PARENT_REMOVE" | "TEXT_METADATA_PATCH" | "DECORATION_PATCH";
type Candidate = "direct" | "packed" | "packed_v3" | "packed_v4";
type Workload = "plain_text_column" | "styled_span_heavy" | "row_heavy" | "column_track_heavy" | "grid_heavy" | "decoration_heavy" | "diff_heavy" | "component_heavy" | "mixed_realistic" | "wide_column_one_edit" | "wide_row_one_edit" | "wide_grid_cell_edit" | "long_text_wrap_only" | "long_text_one_span_edit" | "large_diff_one_hunk_edit" | "large_decoration_only_change";
type Size = { readonly name: string; readonly nodes: number };

type BenchmarkHost = {
  render(view: object): void;
  advanceTime(milliseconds: number): void;
  createViewSlot(initial: object): object;
  dispose(): void;
};
type PackedHost = BenchmarkHost & {
  tuiPerfPackedRender(words: Uint32Array, strings: string[]): void;
  tuiPerfV3PackedRender?: (words: Uint32Array, bytes: Uint8Array) => void;
  tuiPerfV3PackedRenderStrings?: (words: Uint32Array, strings: readonly string[]) => void;
  tuiPerfV3PackedRenderRef?: (generation: number, packedRef: number) => void;
  tuiPerfV4PackedRender?: (words: Uint32Array, bytes: Uint8Array) => void;
  tuiPerfV4PackedRenderRef?: (generation: number, packedRef: number) => void;
};
type PerfNative = typeof native & {
  tuiPerfReset?: () => void;
  tuiPerfSnapshot?: () => Record<string, number>;
  tuiPerfResetViewBridgeCache?: () => void;
  tuiPerfViewBridgeCacheSize?: () => number;
  tuiPerfV3ViewBridgeCacheSize?: () => number;
  tuiPerfV3PackedSlotPages?: () => number;
  tuiPerfV4ViewBridgeCacheSize?: () => number;
};

type Sample = {
  readonly nodeCount: number;
  readonly total: number;
  readonly commit: number;
  readonly forcedFrame: number;
  readonly construction: number;
  readonly encoding: number;
  readonly native: number;
  readonly cpuUserUs: number;
  readonly cpuSystemUs: number;
  readonly heapDelta: number;
  readonly rssDelta: number;
  readonly scratchCapacity: number;
  readonly byteScratchCapacity: number;
  readonly nativeCacheSize: number;
  readonly nativeSemanticCacheSize: number;
  readonly nativePackedSlotPages: number;
};

type Stats = {
  readonly median_ns: number;
  readonly p95_ns: number;
  readonly p99_ns: number;
  readonly median_ci95_ns: readonly [number, number];
  readonly p95_ci95_ns: readonly [number, number];
};

const perfNative = native as PerfNative;
const sizes: readonly Size[] = [
  { name: "small_view", nodes: 20 },
  { name: "medium_view", nodes: 200 },
  { name: "large_view", nodes: 2_000 },
  { name: "huge_view", nodes: 10_000 },
];
const wideSizes: readonly Size[] = [
  { name: "wide_32", nodes: 32 },
  { name: "wide_256", nodes: 256 },
  { name: "wide_2048", nodes: 2_048 },
  { name: "wide_10000", nodes: 10_000 },
  { name: "wide_100000", nodes: 100_000 },
];
const workloads: readonly Workload[] = [
  "plain_text_column", "styled_span_heavy", "row_heavy", "column_track_heavy", "grid_heavy",
  "decoration_heavy", "diff_heavy", "component_heavy", "mixed_realistic",
  "wide_column_one_edit", "wide_row_one_edit", "wide_grid_cell_edit", "long_text_wrap_only",
  "long_text_one_span_edit", "large_diff_one_hunk_edit", "large_decoration_only_change",
];
const patterns: readonly Pattern[] = [
  "COLD", "FIRST_USE", "IDENTICAL_IDENTITY", "SHARED_PATH", "REBUILT_EQUIVALENT", "LARGE_SHARED_SUBTREE_CUTOFF", "SHARED_DEEP",
  "WIDE_PARENT_ONE_EDIT", "WIDE_PARENT_INSERT", "WIDE_PARENT_REMOVE", "TEXT_METADATA_PATCH", "DECORATION_PATCH",
];
const warmupIterations = positiveEnv("PERF_WARMUP", 50);
const measuredIterations = positiveEnv("PERF_MEASURED", 200);
const selectedSizes = filterSizes(sizes, Bun.env.PERF_SIZES);
const selectedWideSizes = filterSizes(wideSizes, Bun.env.PERF_SIZES);
const candidates = filterNames(["direct", "packed", "packed_v3", "packed_v4"] as const, Bun.env.PERF_CANDIDATES);
const selectedWorkloads = filterNames(workloads, Bun.env.PERF_WORKLOADS);
const selectedPatterns = filterNames(patterns, Bun.env.PERF_PATTERNS);
const stringLane = Bun.env.PERF_V3_STRING_LANE === "strings" ? "strings" : "utf8" as const;
if (Bun.env.PERF_V3_STRING_LANE !== undefined && Bun.env.PERF_V3_STRING_LANE !== "utf8" && Bun.env.PERF_V3_STRING_LANE !== "strings") {
  throw new Error("PERF_V3_STRING_LANE must be utf8 or strings");
}
const v4Utf8Writer = (Bun.env.PERF_V4_UTF8_WRITER ?? "textencoder") as PackedV4Utf8Writer;
const v4StringDedupe = (Bun.env.PERF_V4_STRING_DEDUPE ?? "content") as PackedV4StringDedupe;
if (v4Utf8Writer !== "textencoder" && v4Utf8Writer !== "buffer") throw new Error("PERF_V4_UTF8_WRITER must be textencoder or buffer");
if (!["content", "identity", "hybrid16", "hybrid32", "hybrid64", "hybrid128"].includes(v4StringDedupe)) throw new Error("PERF_V4_STRING_DEDUPE is invalid");

function positiveEnv(name: string, fallback: number): number {
  const value = Number(Bun.env[name] ?? fallback);
  if (!Number.isSafeInteger(value) || value <= 0) throw new Error(`${name} must be a positive integer`);
  return value;
}

function measurementCount(pattern: Pattern): number {
  if (Bun.env.PERF_MEASURED !== undefined) return measuredIterations;
  if (pattern === "IDENTICAL_IDENTITY" || pattern === "TEXT_METADATA_PATCH" || pattern === "DECORATION_PATCH") return 1_000;
  return measuredIterations;
}

function filterNames<T extends string>(values: readonly T[], filter: string | undefined): readonly T[] {
  if (filter === undefined || filter.trim() === "") return values;
  const selected = new Set(filter.split(",").map((value) => value.trim()));
  return values.filter((value) => selected.has(value));
}

function filterSizes(values: readonly Size[], filter: string | undefined): readonly Size[] {
  if (filter === undefined || filter.trim() === "") return values;
  const selected = new Set(filter.split(",").map((value) => value.trim()));
  return values.filter((value) => selected.has(value.name));
}

function now(): number { return Bun.nanoseconds(); }

function createHost(): PackedHost {
  const Host = native.NativeTuiHost;
  if (Host === undefined) throw new Error("native TUI host is unavailable");
  return new Host(80, 24, true) as unknown as PackedHost;
}

function renderDirect(host: BenchmarkHost, view: View): void {
  host.render(nodeForDirectBridge(view));
}

function tree(nodes: number, prefix = "node"): View {
  const leaves = Math.max(1, nodes - 1);
  return View.vertical((column) => {
    for (let index = 0; index < leaves; index += 1) column.child(View.text(`${prefix}-${index}`));
  });
}

function styledSpanTree(nodes: number): View {
  const leaves = Math.max(1, nodes - 1);
  return View.vertical((column) => {
    for (let index = 0; index < leaves; index += 1) {
      const style = Style.new().bold().foreground(index % 2 === 0 ? "cyan" : "yellow").attribute("italic", index % 3 === 0);
      column.child(View.styledText([
        TextSpan.plain("prefix "),
        TextSpan.styled(`styled-${index}`, style),
        TextSpan.plain(" suffix"),
      ]).wrap(index % 2 === 0 ? "wordThenGrapheme" : "grapheme").textAlign(index % 3 === 0 ? "center" : "start"));
    }
  });
}

function rowTree(nodes: number): View {
  const groups = Math.max(1, Math.ceil((nodes - 1) / 4));
  return View.vertical((column) => {
    for (let index = 0; index < groups; index += 1) {
      column.child(View.horizontal((row) => {
        row.gap(index % 3);
        row.child(View.text(`a-${index}`));
        row.fixed(4, View.text(`b-${index}`));
        row.flex(View.text(`c-${index}`));
      }));
    }
  });
}

function trackTree(nodes: number): View {
  const leaves = Math.max(1, nodes - 1);
  return View.vertical((column) => {
    for (let index = 0; index < leaves; index += 1) {
      if (index % 4 === 0) column.fixed(8, View.text(`fixed-${index}`));
      else if (index % 4 === 1) column.flex(View.text(`flex-${index}`));
      else if (index % 4 === 2) column.flexMax(3, View.text(`flexmax-${index}`));
      else column.contentMax(2, View.text(`contentmax-${index}`));
    }
  });
}

function gridTree(nodes: number): View {
  const rows = Math.max(1, Math.ceil((nodes - 1) / 4));
  return View.grid((grid) => {
    grid.columns([{ kind: "fixed", size: 12 }, { kind: "flex" }, { kind: "contentMax", max: 8 }]);
    grid.columnGap(1).rowGap(1);
    for (let index = 0; index < rows; index += 1) {
      const track = index % 2 === 0 ? { kind: "content" as const } : { kind: "flexMax" as const, max: 4 };
      grid.rowWith(track, (row) => {
        row.cellWith({ columnSpan: 1, horizontalAlign: "start" }, View.text(`grid-a-${index}`));
        row.cellWith({ columnSpan: 2, verticalAlign: "center" }, View.text(`grid-b-${index}`));
      });
    }
  });
}

function decorationTree(nodes: number): View {
  const leaves = Math.max(1, Math.floor((nodes - 1) / 2));
  return View.vertical((column) => {
    for (let index = 0; index < leaves; index += 1) {
      column.child(View.text(`decorated-${index}`)
        .padding(index % 2 === 0 ? 1 : 2)
        .foreground(index % 2 === 0 ? "green" : { type: "ansi", value: 33 })
        .background("theme:surface")
        .border({ style: index % 3 === 0 ? "rounded" : "plain", edges: "topBottom", color: "cyan" })
        .style(Style.new().bold().attribute("dim", index % 2 === 0))
        .styleState("phase", index % 2 === 0 ? "a" : "b")
        .minWidth(1).maxWidth(40).minHeight(0).maxHeight(4));
    }
  });
}

function diffTree(nodes: number): View {
  const hunks = Math.max(1, Math.ceil(nodes / 50));
  const values = [];
  for (let hunk = 0; hunk < hunks; hunk += 1) {
    const start = hunk * 4;
    values.push(new DiffHunk(
      new DiffRange(start, 2),
      new DiffRange(start, 2),
      [DiffLine.context(start + 1, start + 1, `context-${hunk}`), DiffLine.deletion(start + 2, `old-${hunk}`), DiffLine.addition(start + 2, `new-${hunk}`)],
    ));
  }
  return new DiffRenderer().render(values);
}

function componentTree(_nodes: number, componentId: number): View {
  // A host component slot is an identity-bearing registration, not a freely
  // repeatable leaf. Keep one registered component and vary surrounding
  // semantic structure in mixed workloads instead of inventing duplicate
  // registrations with the same native handle.
  return View.vertical((column) => {
    column.child(View.component({ id: componentId as unknown as NativeHandleId }));
    column.child(View.text("component-tail"));
  });
}

function mixedTree(nodes: number): View {
  const each = Math.max(2, Math.floor(nodes / 5));
  return View.vertical((column) => {
    column.child(tree(each, "mixed-text"));
    column.child(styledSpanTree(each));
    column.child(rowTree(each));
    column.child(decorationTree(each));
    column.child(diffTree(each));
  });
}

function wideColumn(nodes: number, prefix = "wide"): View {
  return View.vertical(Array.from({ length: Math.max(1, nodes) }, (_, index) => View.text(`${prefix}-${index}`)));
}

function wideRow(nodes: number, prefix = "wide"): View {
  return View.horizontal(Array.from({ length: Math.max(1, nodes) }, (_, index) => View.text(`${prefix}-${index}`)));
}

function gridCellMutation(nodes: number, index: number): View {
  const rows = Math.max(1, Math.ceil((nodes - 1) / 4));
  const targetRow = Math.floor(rows / 2);
  return View.grid((grid) => {
    grid.columns([{ kind: "fixed", size: 12 }, { kind: "flex" }, { kind: "contentMax", max: 8 }]);
    grid.columnGap(1).rowGap(1);
    for (let rowIndex = 0; rowIndex < rows; rowIndex += 1) {
      const track = rowIndex % 2 === 0 ? { kind: "content" as const } : { kind: "flexMax" as const, max: 4 };
      grid.rowWith(track, (row) => {
        row.cellWith({ columnSpan: 1, horizontalAlign: "start" }, rowIndex === targetRow ? View.text(`grid-edited-${index}`) : View.text(`grid-a-${rowIndex}`));
        row.cellWith({ columnSpan: 2, verticalAlign: "center" }, View.text(`grid-b-${rowIndex}`));
      });
    }
  });
}

function wideParentMutation(workload: Workload, nodes: number, pattern: Pattern, index: number): View {
  if (workload === "wide_grid_cell_edit") return gridCellMutation(nodes, index);
  const values = Array.from({ length: Math.max(1, nodes) }, (_, childIndex) => View.text(`wide-${childIndex}`));
  const position = pattern === "WIDE_PARENT_ONE_EDIT" ? index % values.length : Math.floor(values.length / 2);
  if (pattern === "WIDE_PARENT_ONE_EDIT") values[position] = View.text(`edited-${index}`);
  if (pattern === "WIDE_PARENT_INSERT") values.splice(position, 0, View.text(`inserted-${index}`));
  if (pattern === "WIDE_PARENT_REMOVE") values.splice(position, 1);
  return workload === "wide_row_one_edit" ? View.horizontal(values) : View.vertical(values);
}

function longText(): View { return View.text("x".repeat(8_192)); }

function workloadView(workload: Workload, nodes: number, componentId?: number): View {
  switch (workload) {
    case "plain_text_column": return tree(nodes);
    case "styled_span_heavy": return styledSpanTree(nodes);
    case "row_heavy": return rowTree(nodes);
    case "column_track_heavy": return trackTree(nodes);
    case "grid_heavy": return gridTree(nodes);
    case "decoration_heavy": return decorationTree(nodes);
    case "diff_heavy": return diffTree(nodes);
    case "component_heavy":
      if (componentId === undefined) throw new Error("component workload requires a registered component");
      return componentTree(nodes, componentId);
    case "mixed_realistic": return mixedTree(nodes);
  case "wide_column_one_edit": return wideColumn(nodes);
  case "wide_row_one_edit": return wideRow(nodes);
  case "wide_grid_cell_edit": return gridTree(nodes);
  case "long_text_wrap_only": return longText();
  case "long_text_one_span_edit": return View.styledText([TextSpan.plain("x".repeat(8_192))]);
  case "large_diff_one_hunk_edit": return diffTree(nodes);
  case "large_decoration_only_change": return View.text("decoration").padding(1);
  }
}

function largeSharedSubtreeCutoff(size: number, index: number, shared: View): View {
  return View.vertical((column) => {
    column.child(shared);
    column.child(View.text(`changed-${index}`));
  });
}

function sharedDeep(depth: number, index: number, stable: View): View {
  let view = View.text(`deep-changed-${index}`);
  for (let level = 0; level < depth; level += 1) {
    const child = view;
    view = View.vertical((column) => {
      column.child(stable);
      column.child(child);
    });
  }
  return view;
}

function uniqueNodeCount(view: View): number {
  const seen = new Set<number>();
  const visit = (node: BridgeViewNode): void => {
    if (seen.has(node.id)) return;
    seen.add(node.id);
    switch (node.kind) {
      case BRIDGE_VIEW_KIND.row:
      case BRIDGE_VIEW_KIND.column:
        for (const child of node.children) visit(child.child);
        break;
      case BRIDGE_VIEW_KIND.hanging: visit(node.prefix); visit(node.continuation); visit(node.body); break;
      case BRIDGE_VIEW_KIND.grid:
        for (const row of node.rows) for (const cell of row.cells) visit(cell.view);
        break;
      case BRIDGE_VIEW_KIND.container:
      case BRIDGE_VIEW_KIND.clamp:
      case BRIDGE_VIEW_KIND.contentMax:
      case BRIDGE_VIEW_KIND.decorated: visit(node.child); break;
      default: break;
    }
  };
  visit(nodeForDirectBridge(view));
  return seen.size;
}

function buildModeView(
  workload: Workload,
  size: number,
  pattern: Pattern,
  index: number,
  shared?: View,
  componentId?: number,
): View {
  if (pattern === "SHARED_PATH" || pattern === "LARGE_SHARED_SUBTREE_CUTOFF") return largeSharedSubtreeCutoff(size, index, shared ?? tree(Math.max(2, Math.floor(size / 2)), "shared"));
  if (pattern === "SHARED_DEEP") return sharedDeep(Math.min(64, Math.max(4, Math.floor(Math.log2(size)))), index, shared ?? tree(Math.max(2, Math.floor(size / 3)), "deep-shared"));
  if (pattern === "WIDE_PARENT_ONE_EDIT" || pattern === "WIDE_PARENT_INSERT" || pattern === "WIDE_PARENT_REMOVE") return workloadView(workload, size, componentId);
  if (pattern === "TEXT_METADATA_PATCH") return longText();
  if (pattern === "DECORATION_PATCH") return View.text("decoration").padding(1);
  return workloadView(workload, size, componentId);
}

function percentile(samples: readonly number[], percentage: number): number {
  const sorted = [...samples].sort((left, right) => left - right);
  const index = Math.ceil((sorted.length - 1) * percentage / 100);
  return sorted[index] ?? 0;
}

function bootstrapInterval(samples: readonly number[], percentage: number): readonly [number, number] {
  if (samples.length < 2) return [samples[0] ?? 0, samples[0] ?? 0];
  let seed = 0x7f4a7c15;
  const estimates: number[] = [];
  for (let iteration = 0; iteration < 1_000; iteration += 1) {
    const resample: number[] = [];
    for (let index = 0; index < samples.length; index += 1) {
      seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
      resample.push(samples[seed % samples.length]!);
    }
    estimates.push(percentile(resample, percentage));
  }
  estimates.sort((left, right) => left - right);
  return [estimates[25]!, estimates[974]!];
}

function stats(samples: readonly number[]): Stats {
  return {
    median_ns: percentile(samples, 50),
    p95_ns: percentile(samples, 95),
    p99_ns: percentile(samples, 99),
    median_ci95_ns: bootstrapInterval(samples, 50),
    p95_ci95_ns: bootstrapInterval(samples, 95),
  };
}

function commandText(command: string[]): string {
  const result = Bun.spawnSync(command);
  return new TextDecoder().decode(result.stdout).trim() || "unknown";
}

function gitSha(): string { return commandText(["git", "rev-parse", "HEAD"]); }
function gitDirty(): boolean { const status = commandText(["git", "status", "--porcelain"]); return status !== "unknown" && status !== ""; }
function gitPatchSha256(): string {
  return commandText(["sh", "-c", "{ git diff HEAD --binary; git ls-files --others --exclude-standard -z | xargs -0 -n1 shasum -a 256; } | shasum -a 256"]).split(/\s+/)[0] ?? "unknown";
}
function sha256(path: string): string { return commandText(["shasum", "-a", "256", path]).split(/\s+/)[0] ?? "unknown"; }

function runtimeVersion(command: string): string {
  const result = Bun.spawnSync(command.split(" "));
  return new TextDecoder().decode(result.stdout).trim() || "unknown";
}

function addCounters(target: Record<string, number>, next: Record<string, number>, sign = 1): void {
  for (const [key, value] of Object.entries(next)) target[key] = (target[key] ?? 0) + value * sign;
}

function diffCounters(before: Record<string, number>, after: Record<string, number>): Record<string, number> {
  const result: Record<string, number> = {};
  for (const key of new Set([...Object.keys(before), ...Object.keys(after)])) result[key] = (after[key] ?? 0) - (before[key] ?? 0);
  return result;
}

function nativeCounters(): Record<string, number> { return perfNative.tuiPerfSnapshot?.() ?? {}; }

function createComponentId(host: BenchmarkHost): number | undefined {
  const slot = host.createViewSlot(nodeForBridge(View.spacer(0))) as { componentId?: () => number | null; dispose?: () => void };
  const id = slot.componentId?.();
  if (id === undefined || id === null) return undefined;
  return id;
}

function runSample(
  candidate: Candidate,
  pattern: Pattern,
  workload: Workload,
  size: number,
  index: number,
  state: CaseState,
  warmup: boolean,
): Sample {
  const firstUse = pattern === "FIRST_USE";
  const host = firstUse
    ? createHost()
    : candidate === "direct" ? state.directHost
      : candidate === "packed" ? state.packedHost
        : candidate === "packed_v3" ? state.packedV3Host
          : state.packedV4Host;
  const encoder = candidate === "packed"
    ? (firstUse ? createPackedViewEncoder() : state.packedEncoder)
    : candidate === "packed_v3"
      ? (firstUse ? createPackedV3Encoder(stringLane) : state.packedV3Encoder)
      : candidate === "packed_v4"
        ? (firstUse ? createPackedV4Encoder(v4Utf8Writer, v4StringDedupe) : state.packedV4Encoder)
        : undefined;
  if (pattern === "COLD" && candidate === "packed") (encoder as PackedViewEncoder).resetKnownNativeState();
  const componentId = firstUse && workload === "component_heavy"
    ? createComponentId(host)
    : candidate === "direct" ? state.directComponentId
      : candidate === "packed" ? state.packedComponentId
        : candidate === "packed_v3" ? state.packedV3ComponentId
          : state.packedV4ComponentId;
  const constructionStarted = now();
  const baseView = candidate === "direct" ? state.directBase!
    : candidate === "packed" ? state.packedBase!
      : candidate === "packed_v3" ? state.packedV3Base!
        : state.packedV4Base!;
  const isWideParentMode = pattern === "WIDE_PARENT_ONE_EDIT" || pattern === "WIDE_PARENT_INSERT" || pattern === "WIDE_PARENT_REMOVE";
  let retainedView: View;
  if (isWideParentMode && candidate !== "packed_v3" && candidate !== "packed_v4") {
    retainedView = wideParentMutation(workload, size, pattern, index);
  } else if ((candidate === "packed_v3" || candidate === "packed_v4") && workload === "wide_grid_cell_edit" && pattern === "WIDE_PARENT_ONE_EDIT") {
    const row = Math.floor(Math.max(1, Math.ceil((size - 1) / 4)) / 2);
    retainedView = candidate === "packed_v3"
      ? replacePackedGridCell(state.packedV3Base!, row, 0, View.text(`grid-edited-${index}`))
      : replacePackedV4GridCell(state.packedV4Base!, row, 0, View.text(`grid-edited-${index}`));
  } else if (candidate === "packed_v3" && pattern === "WIDE_PARENT_ONE_EDIT") {
    retainedView = replacePackedAxisChild(state.packedV3Base!, index % Math.max(1, size), View.text(`edited-${index}`));
  } else if (candidate === "packed_v4" && pattern === "WIDE_PARENT_ONE_EDIT") {
    retainedView = replacePackedV4AxisChild(state.packedV4Base!, index % Math.max(1, size), View.text(`edited-${index}`));
  } else if (candidate === "packed_v3" && pattern === "WIDE_PARENT_INSERT") {
    retainedView = splicePackedAxisChildren(state.packedV3Base!, Math.floor(size / 2), 0, [View.text(`inserted-${index}`)]);
  } else if (candidate === "packed_v4" && pattern === "WIDE_PARENT_INSERT") {
    retainedView = splicePackedV4AxisChildren(state.packedV4Base!, Math.floor(size / 2), 0, [View.text(`inserted-${index}`)]);
  } else if (candidate === "packed_v3" && pattern === "WIDE_PARENT_REMOVE") {
    retainedView = splicePackedAxisChildren(state.packedV3Base!, Math.floor(size / 2), 1, []);
  } else if (candidate === "packed_v4" && pattern === "WIDE_PARENT_REMOVE") {
    retainedView = splicePackedV4AxisChildren(state.packedV4Base!, Math.floor(size / 2), 1, []);
  } else {
    retainedView = pattern === "IDENTICAL_IDENTITY" || pattern === "TEXT_METADATA_PATCH" || pattern === "DECORATION_PATCH"
      ? baseView
      : buildModeView(workload, size, pattern, index,
        candidate === "direct" ? state.directShared
          : candidate === "packed" ? state.packedShared
            : candidate === "packed_v3" ? state.packedV3Shared
              : state.packedV4Shared,
        componentId);
  }
  if (pattern === "TEXT_METADATA_PATCH") retainedView = retainedView.noWrap();
  if (pattern === "DECORATION_PATCH") retainedView = retainedView.maxWidth(40);
  const construction = now() - constructionStarted;
  const nativeBefore = nativeCounters();
  const packedBefore = packedEncoderSnapshot();
  const packedV3Before = packedV3Snapshot();
  const packedV4Before = packedV4Snapshot();
  const cpuBefore = process.cpuUsage();
  const heapBefore = process.memoryUsage().heapUsed;
  const rssBefore = process.memoryUsage().rss;
  const commitStarted = now();
  let encoding = 0;
  let native = 0;
  let encodingStarted = 0;
  let nativeStarted = 0;
  if (candidate === "direct") {
    renderDirect(host, retainedView);
    native = now() - commitStarted;
  } else if (candidate === "packed") {
    renderPackedView(encoder as PackedViewEncoder, retainedView, (words, strings) => (host as PackedHost).tuiPerfPackedRender(words, strings), {
      encodingStarted: () => { encodingStarted = now(); },
      encodingFinished: () => { if (encodingStarted !== 0) { encoding += now() - encodingStarted; encodingStarted = 0; } },
      nativeStarted: () => { nativeStarted = now(); },
      nativeFinished: () => { if (nativeStarted !== 0) { native += now() - nativeStarted; nativeStarted = 0; } },
    });
  } else if (candidate === "packed_v3") {
    const packedHost = host as PackedHost;
    if (packedHost.tuiPerfV3PackedRenderRef === undefined
      || (stringLane === "utf8" && packedHost.tuiPerfV3PackedRender === undefined)
      || (stringLane === "strings" && packedHost.tuiPerfV3PackedRenderStrings === undefined)) {
      throw new Error("native addon lacks selected Packed V3 benchmark methods");
    }
    let encodingStarted = 0;
    let nativeStarted = 0;
    renderPackedV3View(
      encoder as PackedV3Encoder,
      retainedView,
      (words, bytes, strings) => {
        if (stringLane === "strings") {
          if (packedHost.tuiPerfV3PackedRenderStrings === undefined) throw new Error("native addon lacks Packed V3 string-lane method");
          packedHost.tuiPerfV3PackedRenderStrings(words, strings);
        } else {
          if (packedHost.tuiPerfV3PackedRender === undefined) throw new Error("native addon lacks Packed V3 byte-lane method");
          packedHost.tuiPerfV3PackedRender(words, bytes);
        }
      },
      (generation, packedRef) => packedHost.tuiPerfV3PackedRenderRef!(generation, packedRef),
      {
        encodingStarted: () => { encodingStarted = now(); },
        encodingFinished: () => { if (encodingStarted !== 0) { encoding += now() - encodingStarted; encodingStarted = 0; } },
        nativeStarted: () => { nativeStarted = now(); },
        nativeFinished: () => { if (nativeStarted !== 0) { native += now() - nativeStarted; nativeStarted = 0; } },
      },
    );
  } else {
    const packedHost = host as PackedHost;
    if (packedHost.tuiPerfV4PackedRenderRef === undefined || packedHost.tuiPerfV4PackedRender === undefined) {
      throw new Error("native addon lacks Packed V4 methods");
    }
    renderPackedV4View(
      encoder as PackedV4Encoder,
      retainedView,
      (words, bytes) => packedHost.tuiPerfV4PackedRender!(words, bytes),
      (generation, packedRef) => packedHost.tuiPerfV4PackedRenderRef!(generation, packedRef),
      {
        encodingStarted: () => { encodingStarted = now(); },
        encodingFinished: () => { if (encodingStarted !== 0) { encoding += now() - encodingStarted; encodingStarted = 0; } },
        nativeStarted: () => { nativeStarted = now(); },
        nativeFinished: () => { if (nativeStarted !== 0) { native += now() - nativeStarted; nativeStarted = 0; } },
      },
    );
  }

  const commit = now() - commitStarted;
  const forcedStarted = commitStarted;
  if (Bun.env.PERF_FORCED_FRAME === "1") host.advanceTime(0);
  const forcedFrame = now() - forcedStarted;
  const cpu = process.cpuUsage(cpuBefore);
  const heapDelta = Math.max(0, process.memoryUsage().heapUsed - heapBefore);
  const rssDelta = Math.max(0, process.memoryUsage().rss - rssBefore);
  const nativeCacheSize = candidate === "packed_v3"
    ? perfNative.tuiPerfV3ViewBridgeCacheSize?.() ?? 0
    : candidate === "packed_v4"
      ? perfNative.tuiPerfV4ViewBridgeCacheSize?.() ?? 0
      : perfNative.tuiPerfViewBridgeCacheSize?.() ?? 0;
  const nativeSemanticCacheSize = perfNative.tuiPerfViewBridgeCacheSize?.() ?? 0;
  const nativePackedSlotPages = candidate === "packed_v3" ? perfNative.tuiPerfV3PackedSlotPages?.() ?? 0 : 0;
  const nativeAfter = nativeCounters();
  const packedAfter = packedEncoderSnapshot();
  const counterTarget = candidate === "direct" ? state.directCounters
    : candidate === "packed" ? state.packedCounters
      : candidate === "packed_v3" ? state.packedV3Counters
        : state.packedV4Counters;
  addCounters(counterTarget, diffCounters(nativeBefore, nativeAfter));
  if (candidate === "packed") addCounters(state.packedCounters, diffCounters(packedBefore, packedAfter));
  if (candidate === "packed_v3") addCounters(state.packedV3Counters, diffCounters(packedV3Before, packedV3Snapshot()));
  if (candidate === "packed_v4") addCounters(state.packedV4Counters, diffCounters(packedV4Before, packedV4Snapshot()));
  if (firstUse) host.dispose();
  if (warmup) return { nodeCount: 0, total: 0, commit: 0, forcedFrame: 0, construction: 0, encoding: 0, native: 0, cpuUserUs: 0, cpuSystemUs: 0, heapDelta: 0, rssDelta: 0, scratchCapacity: 0, byteScratchCapacity: 0, nativeCacheSize: 0, nativeSemanticCacheSize: 0, nativePackedSlotPages: 0 };
  const scratchCapacity = candidate === "packed" ? (encoder as PackedViewEncoder).scratchCapacity()
    : candidate === "packed_v3" ? (encoder as PackedV3Encoder).wordScratchCapacity
      : candidate === "packed_v4" ? (encoder as PackedV4Encoder).wordScratchCapacity
        : 0;
  const byteScratchCapacity = candidate === "packed_v3" ? (encoder as PackedV3Encoder).byteScratchCapacity
    : candidate === "packed_v4" ? (encoder as PackedV4Encoder).byteScratchCapacity
      : 0;
  return { nodeCount: uniqueNodeCount(retainedView), total: construction + commit, commit, forcedFrame: construction + forcedFrame, construction, encoding, native: native || commit, cpuUserUs: cpu.user, cpuSystemUs: cpu.system, heapDelta, rssDelta, scratchCapacity, byteScratchCapacity, nativeCacheSize, nativeSemanticCacheSize, nativePackedSlotPages };
}

type CaseState = {
  readonly directHost: BenchmarkHost;
  readonly packedHost: PackedHost;
  readonly packedV3Host: PackedHost;
  readonly packedV4Host: PackedHost;
  readonly packedEncoder: PackedViewEncoder;
  readonly packedV3Encoder: PackedV3Encoder;
  readonly packedV4Encoder: PackedV4Encoder;
  readonly directCounters: Record<string, number>;
  readonly packedCounters: Record<string, number>;
  readonly packedV3Counters: Record<string, number>;
  readonly packedV4Counters: Record<string, number>;
  readonly directBase?: View;
  readonly packedBase?: View;
  readonly packedV3Base?: View;
  readonly packedV4Base?: View;
  readonly directShared?: View;
  readonly packedShared?: View;
  readonly packedV3Shared?: View;
  readonly packedV4Shared?: View;
  readonly directComponentId?: number;
  readonly packedComponentId?: number;
  readonly packedV3ComponentId?: number;
  readonly packedV4ComponentId?: number;
};

function makeCaseState(workload: Workload, size: number, pattern: Pattern): CaseState {
  const directHost = createHost();
  const packedHost = createHost();
  const packedV3Host = createHost();
  const packedV4Host = createHost();
  const directComponentId = workload === "component_heavy" ? createComponentId(directHost) : undefined;
  const packedComponentId = workload === "component_heavy" ? createComponentId(packedHost) : undefined;
  const packedV3ComponentId = workload === "component_heavy" ? createComponentId(packedV3Host) : undefined;
  const packedV4ComponentId = workload === "component_heavy" ? createComponentId(packedV4Host) : undefined;
  return {
    directHost,
    packedHost,
    packedV3Host,
    packedV4Host,
    packedEncoder: createPackedViewEncoder(),
    packedV3Encoder: createPackedV3Encoder(stringLane),
    packedV4Encoder: createPackedV4Encoder(v4Utf8Writer, v4StringDedupe),
    directCounters: {},
    packedCounters: {},
    packedV3Counters: {},
    packedV4Counters: {},
    directBase: workloadView(workload, size, directComponentId),
    packedBase: workloadView(workload, size, packedComponentId),
    packedV3Base: workloadView(workload, size, packedV3ComponentId),
    packedV4Base: workloadView(workload, size, packedV4ComponentId),
    directShared: pattern === "SHARED_PATH" || pattern === "LARGE_SHARED_SUBTREE_CUTOFF" || pattern === "SHARED_DEEP" ? tree(Math.max(2, Math.floor(size / 2)), "shared-direct") : undefined,
    packedShared: pattern === "SHARED_PATH" || pattern === "LARGE_SHARED_SUBTREE_CUTOFF" || pattern === "SHARED_DEEP" ? tree(Math.max(2, Math.floor(size / 2)), "shared-packed") : undefined,
    packedV3Shared: pattern === "SHARED_PATH" || pattern === "LARGE_SHARED_SUBTREE_CUTOFF" || pattern === "SHARED_DEEP" ? tree(Math.max(2, Math.floor(size / 2)), "shared-packed-v3") : undefined,
    packedV4Shared: pattern === "SHARED_PATH" || pattern === "LARGE_SHARED_SUBTREE_CUTOFF" || pattern === "SHARED_DEEP" ? tree(Math.max(2, Math.floor(size / 2)), "shared-packed-v4") : undefined,
    directComponentId,
    packedComponentId,
    packedV3ComponentId,
    packedV4ComponentId,
  };
}

function primePatchBases(pattern: Pattern, state: CaseState): void {
  if (pattern !== "TEXT_METADATA_PATCH" && pattern !== "DECORATION_PATCH") return;
  if (candidates.includes("direct")) state.directHost.render(nodeForDirectBridge(state.directBase!));
  if (candidates.includes("packed")) {
    const packedTransaction = state.packedEncoder.encodeRoots([nodeForBridge(state.packedBase!)]);
    state.packedHost.tuiPerfPackedRender(packedTransaction.words, packedTransaction.strings);
    state.packedEncoder.commitSuccessfulDefinitions();
  }
  if (candidates.includes("packed_v3")) {
    const host = state.packedV3Host;
    renderPackedV3View(
      state.packedV3Encoder,
      state.packedV3Base!,
      (words, bytes, strings) => {
        if (stringLane === "strings") host.tuiPerfV3PackedRenderStrings!(words, strings);
        else host.tuiPerfV3PackedRender!(words, bytes);
      },
      (generation, packedRef) => host.tuiPerfV3PackedRenderRef!(generation, packedRef),
    );
  }
  if (candidates.includes("packed_v4")) {
    const host = state.packedV4Host;
    renderPackedV4View(
      state.packedV4Encoder,
      state.packedV4Base!,
      (words, bytes) => host.tuiPerfV4PackedRender!(words, bytes),
      (generation, packedRef) => host.tuiPerfV4PackedRenderRef!(generation, packedRef),
    );
  }
}

function emitCase(candidate: Candidate, workload: Workload, size: Size, pattern: Pattern, samples: readonly Sample[], state: CaseState, sha: string): void {
  const total = samples.map((sample) => sample.total);
  const commit = samples.map((sample) => sample.commit);
  const forced = samples.map((sample) => sample.forcedFrame);
  const construction = samples.map((sample) => sample.construction);
  const encoding = samples.map((sample) => sample.encoding);
  const native = samples.map((sample) => sample.native);
  const nodeCount = samples.find((sample) => sample.nodeCount > 0)?.nodeCount ?? size.nodes;
  const output = {
    benchmark_version: "PERF-9",
    candidate,
    workload,
    size: size.name,
    mode: pattern,
    node_count: nodeCount,
    git_sha: sha,
    git_dirty: gitDirty(),
    git_patch_sha256: gitDirty() ? gitPatchSha256() : undefined,
    protocol_version: candidate === "packed_v4" ? 4 : candidate === "packed_v3" ? 2 : candidate === "packed" ? 1 : 0,
    bridge_schema_version: 1,
    string_lane: candidate === "packed_v4" ? "S1_UTF8_ARENA" : stringLane === "utf8" ? "S1_UTF8_ARENA" : "S2_MOVE_ONCE_STRINGS",
    configured_string_lane: stringLane === "utf8" ? "S1_UTF8_ARENA" : "S2_MOVE_ONCE_STRINGS",
    utf8_writer: candidate === "packed_v4" ? v4Utf8Writer : candidate === "packed_v3" && stringLane === "utf8" ? "TextEncoder.encodeInto" : null,
    string_dedupe_policy: candidate === "packed_v4" ? v4StringDedupe : candidate === "packed_v3" ? "D0_content_all" : null,
    native_string_storage: candidate === "packed_v4" ? "R0_per_string_owned" : candidate === "packed_v3" ? "per_use_owned" : null,
    slab_page_bytes: null,
    native_artifact_sha256: sha256("packages/iyon-runtime/native/iyon-native.node"),
    benchmark_source_sha256: sha256("packages/iyon-runtime/bench/tui_performance.ts"),
    warmup_iterations: warmupIterations,
    measured_iterations: samples.length,
    samples_ns: total,
    commit_samples_ns: commit,
    forced_frame_samples_ns: forced,
    construction_samples_ns: construction,
    encoding_samples_ns: encoding,
    native_samples_ns: native,
    ...stats(total),
    commit_median_ns: percentile(commit, 50),
    commit_p95_ns: percentile(commit, 95),
    forced_frame_median_ns: percentile(forced, 50),
    forced_frame_enabled: Bun.env.PERF_FORCED_FRAME === "1",
    construction_median_ns: percentile(construction, 50),
    encoding_median_ns: percentile(encoding, 50),
    native_median_ns: percentile(native, 50),
    cpu_user_us: samples.reduce((sum, sample) => sum + sample.cpuUserUs, 0),
    cpu_system_us: samples.reduce((sum, sample) => sum + sample.cpuSystemUs, 0),
    heap_peak_delta_bytes: samples.reduce((peak, sample) => Math.max(peak, sample.heapDelta), 0),
    rss_peak_delta_bytes: samples.reduce((peak, sample) => Math.max(peak, sample.rssDelta), 0),
    scratch_word_capacity: samples.reduce((peak, sample) => Math.max(peak, sample.scratchCapacity), 0),
    scratch_byte_capacity: samples.reduce((peak, sample) => Math.max(peak, sample.byteScratchCapacity), 0),
    native_bridge_cache_entries: samples.length === 0 ? 0 : samples[samples.length - 1]!.nativeCacheSize,
    native_semantic_cache_entries: samples.length === 0 ? 0 : samples[samples.length - 1]!.nativeSemanticCacheSize,
    packed_slot_pages_peak: samples.reduce((peak, sample) => Math.max(peak, sample.nativePackedSlotPages), 0),
    counters: candidate === "direct" ? state.directCounters
      : candidate === "packed" ? state.packedCounters
        : candidate === "packed_v3" ? state.packedV3Counters
          : state.packedV4Counters,
    bun_version: Bun.version,
    rustc_version: runtimeVersion("rustc --version"),
    target: `${process.platform}-${process.arch}`,
    profile: "release",
    p99_informational: samples.length < 1_000,
    synthetic_trace: false,
  };
  console.log(JSON.stringify(output));
}

function candidateOrder(index: number): readonly Candidate[] {
  if (candidates.length === 0) throw new Error("PERF_CANDIDATES selected no candidates");
  const offset = index % candidates.length;
  return [...candidates.slice(offset), ...candidates.slice(0, offset)];
}

function runCase(workload: Workload, size: Size, pattern: Pattern, sha: string): void {
  const state = makeCaseState(workload, size.nodes, pattern);
  primePatchBases(pattern, state);
  resetPackedEncoderCounters();
  resetPackedV3Counters();
  resetPackedV4Counters();
  perfNative.tuiPerfReset?.();
  for (let index = 0; index < warmupIterations; index += 1) {
    for (const candidate of candidateOrder(index)) runSample(candidate, pattern, workload, size.nodes, index, state, true);
  }
  resetPackedEncoderCounters();
  resetPackedV3Counters();
  resetPackedV4Counters();
  perfNative.tuiPerfReset?.();
  const samples = new Map<Candidate, Sample[]>(candidates.map((candidate) => [candidate, []]));
  const sampleCount = measurementCount(pattern);
  for (let index = 0; index < sampleCount; index += 1) {
    for (const candidate of candidateOrder(index + warmupIterations)) samples.get(candidate)!.push(runSample(candidate, pattern, workload, size.nodes, index + warmupIterations, state, false));
  }
  for (const candidate of candidates) emitCase(candidate, workload, size, pattern, samples.get(candidate)!, state, sha);
  state.directHost.dispose();
  state.packedHost.dispose();
  state.packedV3Host.dispose();
  state.packedV4Host.dispose();
}

function runSyntheticTrace(sha: string): void {
  const workload: Workload = "mixed_realistic";
  const size: Size = { name: "synthetic_trace", nodes: 2_000 };
  const state = makeCaseState(workload, size.nodes, "LARGE_SHARED_SUBTREE_CUTOFF");
  const totals = new Map<Candidate, number[]>(candidates.map((candidate) => [candidate, []]));
  resetPackedEncoderCounters();
  resetPackedV3Counters();
  resetPackedV4Counters();
  perfNative.tuiPerfReset?.();
  for (let index = 0; index < 1_000; index += 1) {
    const traceSlot = index % 50;
    const mode: Pattern = traceSlot < 35
      ? "LARGE_SHARED_SUBTREE_CUTOFF"
      : traceSlot < 45
        ? "IDENTICAL_IDENTITY"
        : traceSlot < 49
          ? "REBUILT_EQUIVALENT"
          : "COLD";
    for (const candidate of candidateOrder(index)) totals.get(candidate)!.push(runSample(candidate, mode, workload, size.nodes, index, state, false).total);
  }
  const output: Record<string, unknown> = { benchmark_version: "PERF-9", benchmark: "synthetic_trace", synthetic_trace: true, mix: "70% LARGE_SHARED_SUBTREE_CUTOFF, 20% IDENTICAL_IDENTITY, 8% REBUILT_EQUIVALENT, 2% COLD", git_sha: sha, git_dirty: gitDirty(), git_patch_sha256: gitDirty() ? gitPatchSha256() : undefined, protocol_version: 4, bridge_schema_version: 1, string_lane: "S1_UTF8_ARENA", configured_string_lane: stringLane === "utf8" ? "S1_UTF8_ARENA" : "S2_MOVE_ONCE_STRINGS", utf8_writer: v4Utf8Writer, string_dedupe_policy: v4StringDedupe, native_string_storage: "R0_per_string_owned", slab_page_bytes: null, native_artifact_sha256: sha256("packages/iyon-runtime/native/iyon-native.node"), benchmark_source_sha256: sha256("packages/iyon-runtime/bench/tui_performance.ts") };
  for (const candidate of candidates) {
    const values = totals.get(candidate)!;
    output[`${candidate}_total_commit_ns`] = values.reduce((sum, value) => sum + value, 0);
    output[`${candidate}_samples_ns`] = values;
  }
  console.log(JSON.stringify(output));
  state.directHost.dispose();
  state.packedHost.dispose();
  state.packedV3Host.dispose();
}

const sha = gitSha();
if (gitDirty() && Bun.env.PERF_ALLOW_DIRTY !== "1") {
  throw new Error("PERF-8 authoritative runs require a clean worktree; set PERF_ALLOW_DIRTY=1 only for explicitly non-authoritative development runs");
}
for (const workload of selectedWorkloads) {
  const isWideAxisWorkload = workload === "wide_column_one_edit" || workload === "wide_row_one_edit" || workload === "wide_grid_cell_edit";
  const caseSizes = isWideAxisWorkload ? selectedWideSizes : selectedSizes;
  for (const size of caseSizes) {
    for (const pattern of selectedPatterns) {
      const isWideParentMode = pattern === "WIDE_PARENT_ONE_EDIT" || pattern === "WIDE_PARENT_INSERT" || pattern === "WIDE_PARENT_REMOVE";
      if (isWideParentMode && !isWideAxisWorkload) continue;
      if (workload === "wide_grid_cell_edit" && pattern !== "WIDE_PARENT_ONE_EDIT" && isWideParentMode) continue;
      if (pattern === "TEXT_METADATA_PATCH" && workload !== "long_text_wrap_only") continue;
      if (pattern === "DECORATION_PATCH" && workload !== "large_decoration_only_change") continue;
      runCase(workload, size, pattern, sha);
    }
  }
}
if (Bun.env.PERF_TRACE === "1") runSyntheticTrace(sha);

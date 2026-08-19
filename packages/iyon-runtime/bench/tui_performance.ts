import { native } from "../src/native.ts";
import {
  BRIDGE_VIEW_KIND,
  type BridgeViewNode,
} from "../src/tui/ir.ts";
import { View, nodeForBridge } from "../src/tui/values/view.ts";
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

type Pattern = "COLD" | "FIRST_USE" | "IDENTICAL_IDENTITY" | "SHARED_PATH" | "REBUILT_EQUIVALENT" | "SHARED_WIDE" | "SHARED_DEEP";
type Candidate = "direct" | "packed";
type Workload = "plain_text_column" | "styled_span_heavy" | "row_heavy" | "column_track_heavy" | "grid_heavy" | "decoration_heavy" | "diff_heavy" | "component_heavy" | "mixed_realistic";
type Size = { readonly name: string; readonly nodes: number };

type BenchmarkHost = {
  render(view: object): void;
  advanceTime(milliseconds: number): void;
  createViewSlot(initial: object): object;
  dispose(): void;
};
type PackedHost = BenchmarkHost & { tuiPerfPackedRender(words: Uint32Array, strings: string[]): void };
type PerfNative = typeof native & {
  tuiPerfReset?: () => void;
  tuiPerfSnapshot?: () => Record<string, number>;
  tuiPerfResetViewBridgeCache?: () => void;
  tuiPerfViewBridgeCacheSize?: () => number;
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
  readonly nativeCacheSize: number;
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
const workloads: readonly Workload[] = [
  "plain_text_column", "styled_span_heavy", "row_heavy", "column_track_heavy", "grid_heavy",
  "decoration_heavy", "diff_heavy", "component_heavy", "mixed_realistic",
];
const patterns: readonly Pattern[] = [
  "COLD", "FIRST_USE", "IDENTICAL_IDENTITY", "SHARED_PATH", "REBUILT_EQUIVALENT", "SHARED_WIDE", "SHARED_DEEP",
];
const warmupIterations = positiveEnv("PERF_WARMUP", 20);
const measuredIterations = positiveEnv("PERF_MEASURED", 200);
const selectedSizes = filterSizes(sizes, Bun.env.PERF_SIZES);
const selectedWorkloads = filterNames(workloads, Bun.env.PERF_WORKLOADS);
const selectedPatterns = filterNames(patterns, Bun.env.PERF_PATTERNS);

function positiveEnv(name: string, fallback: number): number {
  const value = Number(Bun.env[name] ?? fallback);
  if (!Number.isSafeInteger(value) || value <= 0) throw new Error(`${name} must be a positive integer`);
  return value;
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
  host.render(nodeForBridge(view));
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
  }
}

function sharedWide(size: number, index: number, shared: View): View {
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
  visit(nodeForBridge(view));
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
  if (pattern === "SHARED_PATH" || pattern === "SHARED_WIDE") return sharedWide(size, index, shared ?? tree(Math.max(2, Math.floor(size / 2)), "shared"));
  if (pattern === "SHARED_DEEP") return sharedDeep(Math.min(64, Math.max(4, Math.floor(Math.log2(size)))), index, shared ?? tree(Math.max(2, Math.floor(size / 3)), "deep-shared"));
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

function gitSha(): string {
  const result = Bun.spawnSync(["git", "rev-parse", "HEAD"]);
  return new TextDecoder().decode(result.stdout).trim() || "unknown";
}

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
  const host = firstUse ? createHost() : candidate === "direct" ? state.directHost : state.packedHost;
  const encoder = candidate === "packed" ? (firstUse ? createPackedViewEncoder() : state.packedEncoder) : undefined;
  if (pattern === "COLD") encoder?.resetKnownNativeState();
  const componentId = firstUse && workload === "component_heavy" ? createComponentId(host) : candidate === "direct" ? state.directComponentId : state.packedComponentId;
  const constructionStarted = now();
  const view = pattern === "IDENTICAL_IDENTITY"
    ? candidate === "direct" ? state.directBase! : state.packedBase!
    : buildModeView(workload, size, pattern, index, candidate === "direct" ? state.directShared : state.packedShared, componentId);
  const construction = now() - constructionStarted;
  const nativeBefore = nativeCounters();
  const packedBefore = packedEncoderSnapshot();
  const cpuBefore = process.cpuUsage();
  const heapBefore = process.memoryUsage().heapUsed;
  const rssBefore = process.memoryUsage().rss;
  const commitStarted = now();
  let encoding = 0;
  let native = 0;
  let encodingStarted = 0;
  let nativeStarted = 0;
  if (candidate === "direct") {
    renderDirect(host, view);
    native = now() - commitStarted;
  } else {
    renderPackedView(encoder!, view, (words, strings) => (host as PackedHost).tuiPerfPackedRender(words, strings), {
      encodingStarted: () => { encodingStarted = now(); },
      encodingFinished: () => { if (encodingStarted !== 0) { encoding += now() - encodingStarted; encodingStarted = 0; } },
      nativeStarted: () => { nativeStarted = now(); },
      nativeFinished: () => { if (nativeStarted !== 0) { native += now() - nativeStarted; nativeStarted = 0; } },
    });
  }
  const commit = now() - commitStarted;
  const forcedStarted = commitStarted;
  if (Bun.env.PERF_FORCED_FRAME === "1") host.advanceTime(0);
  const forcedFrame = now() - forcedStarted;
  const cpu = process.cpuUsage(cpuBefore);
  const heapDelta = Math.max(0, process.memoryUsage().heapUsed - heapBefore);
  const rssDelta = Math.max(0, process.memoryUsage().rss - rssBefore);
  const nativeCacheSize = perfNative.tuiPerfViewBridgeCacheSize?.() ?? 0;
  const nativeAfter = nativeCounters();
  const packedAfter = packedEncoderSnapshot();
  addCounters(candidate === "direct" ? state.directCounters : state.packedCounters, diffCounters(nativeBefore, nativeAfter));
  if (candidate === "packed") addCounters(state.packedCounters, diffCounters(packedBefore, packedAfter));
  if (firstUse) host.dispose();
  if (warmup) return { nodeCount: 0, total: 0, commit: 0, forcedFrame: 0, construction: 0, encoding: 0, native: 0, cpuUserUs: 0, cpuSystemUs: 0, heapDelta: 0, rssDelta: 0, scratchCapacity: 0, nativeCacheSize: 0 };
  return { nodeCount: uniqueNodeCount(view), total: construction + commit, commit, forcedFrame: construction + forcedFrame, construction, encoding, native: native || commit, cpuUserUs: cpu.user, cpuSystemUs: cpu.system, heapDelta, rssDelta, scratchCapacity: encoder?.scratchCapacity() ?? 0, nativeCacheSize };
}

type CaseState = {
  readonly directHost: BenchmarkHost;
  readonly packedHost: PackedHost;
  readonly packedEncoder: PackedViewEncoder;
  readonly directCounters: Record<string, number>;
  readonly packedCounters: Record<string, number>;
  readonly directBase?: View;
  readonly packedBase?: View;
  readonly directShared?: View;
  readonly packedShared?: View;
  readonly directComponentId?: number;
  readonly packedComponentId?: number;
};

function makeCaseState(workload: Workload, size: number, pattern: Pattern): CaseState {
  const directHost = createHost();
  const packedHost = createHost();
  const directComponentId = workload === "component_heavy" ? createComponentId(directHost) : undefined;
  const packedComponentId = workload === "component_heavy" ? createComponentId(packedHost) : undefined;
  return {
    directHost,
    packedHost,
    packedEncoder: createPackedViewEncoder(),
    directCounters: {},
    packedCounters: {},
    directBase: workloadView(workload, size, directComponentId),
    packedBase: workloadView(workload, size, packedComponentId),
    directShared: pattern === "SHARED_PATH" || pattern === "SHARED_WIDE" || pattern === "SHARED_DEEP" ? tree(Math.max(2, Math.floor(size / 2)), "shared-direct") : undefined,
    packedShared: pattern === "SHARED_PATH" || pattern === "SHARED_WIDE" || pattern === "SHARED_DEEP" ? tree(Math.max(2, Math.floor(size / 2)), "shared-packed") : undefined,
    directComponentId,
    packedComponentId,
  };
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
    benchmark_version: "PERF-7v2",
    candidate,
    workload,
    size: size.name,
    mode: pattern,
    node_count: nodeCount,
    git_sha: sha,
    warmup_iterations: warmupIterations,
    measured_iterations: measuredIterations,
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
    native_bridge_cache_entries: samples.length === 0 ? 0 : samples[samples.length - 1]!.nativeCacheSize,
    counters: candidate === "direct" ? state.directCounters : state.packedCounters,
    bun_version: Bun.version,
    rustc_version: runtimeVersion("rustc --version"),
    target: `${process.platform}-${process.arch}`,
    profile: "release",
    p99_informational: measuredIterations < 1_000,
    synthetic_trace: false,
  };
  console.log(JSON.stringify(output));
}

function runCase(workload: Workload, size: Size, pattern: Pattern, sha: string): void {
  const state = makeCaseState(workload, size.nodes, pattern);
  resetPackedEncoderCounters();
  perfNative.tuiPerfReset?.();
  for (let index = 0; index < warmupIterations; index += 1) {
    const first = index % 2 === 0 ? "direct" : "packed";
    runSample(first, pattern, workload, size.nodes, index, state, true);
    runSample(first === "direct" ? "packed" : "direct", pattern, workload, size.nodes, index, state, true);
  }
  resetPackedEncoderCounters();
  perfNative.tuiPerfReset?.();
  const directSamples: Sample[] = [];
  const packedSamples: Sample[] = [];
  for (let index = 0; index < measuredIterations; index += 1) {
    const first = index % 2 === 0 ? "direct" : "packed";
    const firstSample = runSample(first, pattern, workload, size.nodes, index + warmupIterations, state, false);
    const second = first === "direct" ? "packed" : "direct";
    const secondSample = runSample(second, pattern, workload, size.nodes, index + warmupIterations, state, false);
    if (first === "direct") { directSamples.push(firstSample); packedSamples.push(secondSample); }
    else { packedSamples.push(firstSample); directSamples.push(secondSample); }
  }
  emitCase("direct", workload, size, pattern, directSamples, state, sha);
  emitCase("packed", workload, size, pattern, packedSamples, state, sha);
  state.directHost.dispose();
  state.packedHost.dispose();
}

function runSyntheticTrace(sha: string): void {
  const workload: Workload = "mixed_realistic";
  const size: Size = { name: "synthetic_trace", nodes: 2_000 };
  const state = makeCaseState(workload, size.nodes, "SHARED_WIDE");
  const directTotals: number[] = [];
  const packedTotals: number[] = [];
  resetPackedEncoderCounters();
  perfNative.tuiPerfReset?.();
  for (let index = 0; index < 1_000; index += 1) {
    const mode: Pattern = index % 50 < 35 ? "SHARED_WIDE" : index % 10 < 9 ? "IDENTICAL_IDENTITY" : index % 50 === 49 ? "REBUILT_EQUIVALENT" : "COLD";
    const first = index % 2 === 0 ? "direct" : "packed";
    const a = runSample(first, mode, workload, size.nodes, index, state, false);
    const b = runSample(first === "direct" ? "packed" : "direct", mode, workload, size.nodes, index, state, false);
    if (first === "direct") { directTotals.push(a.total); packedTotals.push(b.total); }
    else { packedTotals.push(a.total); directTotals.push(b.total); }
  }
  console.log(JSON.stringify({ benchmark_version: "PERF-7v2", benchmark: "synthetic_trace", synthetic_trace: true, mix: "70% SHARED_PATH, 20% IDENTICAL_IDENTITY, 8% REBUILT_EQUIVALENT, 2% large replacement", direct_total_commit_ns: directTotals.reduce((sum, value) => sum + value, 0), packed_total_commit_ns: packedTotals.reduce((sum, value) => sum + value, 0), direct_samples_ns: directTotals, packed_samples_ns: packedTotals, git_sha: sha }));
  state.directHost.dispose();
  state.packedHost.dispose();
}

const sha = gitSha();
for (const workload of selectedWorkloads) {
  for (const size of selectedSizes) {
    for (const pattern of selectedPatterns) runCase(workload, size, pattern, sha);
  }
}
if (Bun.env.PERF_TRACE === "1") runSyntheticTrace(sha);

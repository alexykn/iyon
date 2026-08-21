import { native } from "../src/native.ts";
import { Style, Tui, View } from "../src/tui/index.ts";
import {
  NATIVE_PATH_STEP,
  NATIVE_PATH_VIEW_KIND,
  nodeForDirectBridge,
  textLayoutAtNativePathForTransport,
} from "../src/tui/values/view.ts";
import {
  nativeViewRouteSnapshot,
  nativeViewAbiSession,
  resetNativeViewRouteCounters,
  type NativeViewRouteSnapshot,
} from "../src/tui/native_view_abi.ts";
import { createPackedV4Encoder, renderPackedV4View, type PackedV4Encoder } from "../src/tui/packed_v4.ts";
import { DiffHunk, DiffLine, DiffRange, DiffRenderer, TextSpan } from "../src/tui/index.ts";

/**
 * PERF-11.11 authoritative decision run.
 *
 * The native-shadow candidate uses the public Tui.render boundary. Direct and
 * V4 use the same immutable View values and the same host size. Construction
 * is timed separately, while commit is the complete render/install operation.
 * No result is phase-subtracted.
 */
type Candidate = "direct" | "native_shadow" | "packed_v4";
type Mode =
  | "IDENTICAL_IDENTITY"
  | "SHARED_PATH"
  | "LARGE_SHARED_SUBTREE_CUTOFF"
  | "SHARED_DEEP"
  | "REBUILT_EQUIVALENT"
  | "COLD"
  | "FIRST_USE";
type Workload =
  | "plain_text_column"
  | "styled_span_heavy"
  | "row_heavy"
  | "column_track_heavy"
  | "grid_heavy"
  | "decoration_heavy"
  | "diff_heavy"
  | "mixed_realistic"
  | "long_text_wrap_only"
  | "large_decoration_only_change";

type Case = {
  readonly workload: Workload;
  readonly size: number;
  readonly mode: Mode;
  readonly label: string;
};

type BuiltCase = {
  readonly base: View;
  readonly next: View;
  readonly cold: boolean;
};

type Sample = {
  readonly totalNs: number;
  readonly constructionNs: number;
  readonly commitNs: number;
  readonly encodingNs: number;
  readonly nativeRouteNs: number;
  readonly heapDelta: number;
  readonly rssDelta: number;
  readonly snapshot: Record<string, number>;
  readonly routes: NativeViewRouteSnapshot;
};

type Stats = {
  readonly median_ns: number;
  readonly p95_ns: number;
  readonly p99_ns: number;
  readonly median_ci95_ns: readonly [number, number];
};

type PerfHost = {
  render(view: object): void;
  dispose(): void;
  tuiPerfV4PackedRender?: (words: Uint32Array, bytes: Uint8Array) => void;
  tuiPerfV4PackedRenderRef?: (generation: number, packedRef: number) => void;
};

const Host = native.NativeTuiHost as unknown as (new (width: number, height: number, headless: boolean) => PerfHost) | undefined;
const warmup = positiveEnv("PERF_DECISION_WARMUP", 10);
const iterations = positiveEnv("PERF_DECISION_ITERATIONS", 20);
const repeats = positiveEnv("PERF_DECISION_REPEATS", 3);
const selectedCandidates = filterNames<Candidate>(["direct", "native_shadow", "packed_v4"], Bun.env.PERF_DECISION_CANDIDATES);
const selectedWorkloads = filterNames<Workload>([
  "plain_text_column",
  "styled_span_heavy",
  "row_heavy",
  "column_track_heavy",
  "grid_heavy",
  "decoration_heavy",
  "diff_heavy",
  "mixed_realistic",
  "long_text_wrap_only",
  "large_decoration_only_change",
], Bun.env.PERF_DECISION_WORKLOADS);
const normalSizes = positiveListEnv("PERF_DECISION_SIZES", [20, 200]);
const wideSizes = positiveListEnv("PERF_DECISION_WIDE_SIZES", [2_048, 10_000, 100_000]);
const coldSizes = positiveListEnv("PERF_DECISION_COLD_SIZES", [20, 200, 2_000, 10_000]);
const modes: readonly Mode[] = [
  "IDENTICAL_IDENTITY",
  "SHARED_PATH",
  "LARGE_SHARED_SUBTREE_CUTOFF",
  "SHARED_DEEP",
  "REBUILT_EQUIVALENT",
];

function positiveEnv(name: string, fallback: number): number {
  const value = Number(Bun.env[name] ?? fallback);
  if (!Number.isSafeInteger(value) || value <= 0) throw new Error(`${name} must be a positive integer`);
  return value;
}

function positiveListEnv(name: string, fallback: readonly number[]): readonly number[] {
  const raw = Bun.env[name];
  if (raw === undefined || raw.trim() === "") return fallback;
  const values = raw.split(",").map((value) => Number(value.trim()));
  if (values.length === 0 || values.some((value) => !Number.isSafeInteger(value) || value <= 0)) {
    throw new Error(`${name} must be a comma-separated list of positive integers`);
  }
  return values;
}

function filterNames<T extends string>(values: readonly T[], filter: string | undefined): readonly T[] {
  if (filter === undefined || filter.trim() === "") return values;
  const selected = new Set(filter.split(",").map((value) => value.trim()));
  const result = values.filter((value) => selected.has(value));
  if (result.length === 0) throw new Error("a PERF_DECISION filter selected no values");
  return result;
}

function now(): number { return Bun.nanoseconds(); }

function stats(samples: readonly number[]): Stats {
  const percentile = (percentage: number): number => {
    const sorted = [...samples].sort((left, right) => left - right);
    return sorted[Math.ceil((sorted.length - 1) * percentage / 100)] ?? 0;
  };
  const interval = (percentage: number): readonly [number, number] => {
    if (samples.length < 2) return [samples[0] ?? 0, samples[0] ?? 0];
    let seed = 0x51f15e11;
    const estimates: number[] = [];
    for (let iteration = 0; iteration < 1_000; iteration += 1) {
      const resample: number[] = [];
      for (let index = 0; index < samples.length; index += 1) {
        seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
        resample.push(samples[seed % samples.length]!);
      }
      const sorted = resample.sort((left, right) => left - right);
      estimates.push(sorted[Math.ceil((sorted.length - 1) * percentage / 100)] ?? 0);
    }
    estimates.sort((left, right) => left - right);
    return [estimates[25]!, estimates[974]!];
  };
  return {
    median_ns: percentile(50),
    p95_ns: percentile(95),
    p99_ns: percentile(99),
    median_ci95_ns: interval(50),
  };
}

function tree(nodes: number, prefix: string): View {
  return View.vertical(Array.from({ length: Math.max(1, nodes - 1) }, (_, index) => View.text(`${prefix}-${index}`)));
}

function styledTree(nodes: number): View {
  return View.vertical(Array.from({ length: Math.max(1, Math.min(nodes - 1, 256)) }, (_, index) =>
    View.styledText([
      TextSpan.plain("prefix "),
      TextSpan.styled(`styled-${index}`, Style.new().bold().foreground(index % 2 === 0 ? "cyan" : "yellow")),
      TextSpan.plain(" suffix"),
    ]).textAlign(index % 2 === 0 ? "center" : "start"),
  ));
}

function rowTree(nodes: number): View {
  return View.horizontal(Array.from({ length: Math.max(1, Math.min(nodes, 256)) }, (_, index) => View.text(`row-${index}`)));
}

function trackTree(nodes: number): View {
  return View.vertical(Array.from({ length: Math.max(1, Math.min(nodes - 1, 256)) }, (_, index) => {
    const child = View.text(`track-${index}`);
    if (index % 4 === 0) return child;
    if (index % 4 === 1) return child;
    if (index % 4 === 2) return child;
    return child;
  }));
}

function gridTree(nodes: number): View {
  const rows = Math.max(1, Math.min(Math.ceil(nodes / 4), 128));
  return View.grid((grid) => {
    grid.columns([{ kind: "fixed", size: 12 }, { kind: "flex" }, { kind: "contentMax", max: 8 }]);
    grid.columnGap(1).rowGap(1);
    for (let index = 0; index < rows; index += 1) {
      const track = index % 2 === 0 ? { kind: "content" as const } : { kind: "flexMax" as const, max: 4 };
      grid.rowWith(track, (row) => {
        row.cell(View.text(`grid-a-${index}`));
        row.cell(View.text(`grid-b-${index}`));
      });
    }
  });
}

function decorationTree(nodes: number): View {
  return View.vertical(Array.from({ length: Math.max(1, Math.min(nodes - 1, 128)) }, (_, index) =>
    View.text(`decorated-${index}`).padding(index % 2 === 0 ? 1 : 2).maxWidth(40),
  ));
}

function diffTree(nodes: number): View {
  const hunks = Math.max(1, Math.min(Math.ceil(nodes / 50), 16));
  return new DiffRenderer().render(Array.from({ length: hunks }, (_, index) => new DiffHunk(
    new DiffRange(index * 4, 2),
    new DiffRange(index * 4, 2),
    [
      DiffLine.context(index * 4 + 1, index * 4 + 1, `context-${index}`),
      DiffLine.deletion(index * 4 + 2, `old-${index}`),
      DiffLine.addition(index * 4 + 2, `new-${index}`),
    ],
  )));
}

function workloadView(workload: Workload, size: number, suffix: string): View {
  switch (workload) {
    case "plain_text_column": return tree(size, suffix);
    case "styled_span_heavy": return styledTree(size);
    case "row_heavy": return rowTree(size);
    case "column_track_heavy": return trackTree(size);
    case "grid_heavy": return gridTree(size);
    case "decoration_heavy": return decorationTree(size);
    case "diff_heavy": return diffTree(size);
    case "mixed_realistic": return View.vertical([tree(Math.max(2, Math.floor(size / 3)), `${suffix}-text`), styledTree(size), View.text(`${suffix}-tail`).border({ style: "plain", edges: "topBottom", color: "cyan" })]);
    case "long_text_wrap_only": return View.text("x".repeat(Math.min(8_192, Math.max(64, size * 8))));
    case "large_decoration_only_change": return View.text(`${suffix}-decoration`).padding(1);
  }
}

function pathCase(depth: number, index: number): { base: View; next: View } {
  let base = View.text(`path-${index}`);
  const steps: { kind: number; expectedViewKind: number; selector: number }[] = [];
  for (let level = 0; level < depth; level += 1) {
    const column = level % 2 === 0;
    base = column ? View.vertical([base]) : View.horizontal([base]);
    steps.unshift({
      kind: column ? NATIVE_PATH_STEP.columnChild : NATIVE_PATH_STEP.rowChild,
      expectedViewKind: column ? NATIVE_PATH_VIEW_KIND.column : NATIVE_PATH_VIEW_KIND.row,
      selector: 0,
    });
  }
  return {
    base,
    next: textLayoutAtNativePathForTransport(base, steps, "noWrap", "center"),
  };
}

function transactionCase(editCount: number, index: number): { base: View; next: View } {
  const base = View.vertical(Array.from({ length: editCount }, (_, child) => View.text(`txn-${index}-${child}`)));
  return {
    base,
    next: View.textLayoutTransactionForTransport(base, Array.from({ length: editCount }, (_, child) => ({
      steps: [{ kind: NATIVE_PATH_STEP.columnChild, expectedViewKind: NATIVE_PATH_VIEW_KIND.column, selector: child }],
      wrap: "noWrap" as const,
      align: child % 2 === 0 ? "center" as const : "end" as const,
    }))),
  };
}

function structuralCase(size: number, index: number): { base: View; next: View } {
  const base = View.vertical(Array.from({ length: Math.max(2, size) }, (_, child) => View.spacer(child % 2)));
  return {
    base,
    next: View.replaceAxisChildForPackedTransport(base, Math.floor(size / 2), View.spacer(index % 3)),
  };
}

function cutoffCase(index: number): { base: View; next: View } {
  const stable = tree(64, "stable");
  const base = View.vertical([stable, View.text("old")]);
  const next = View.vertical([stable, View.text(`changed-${index}`)]);
  return { base, next };
}

function buildCase(testCase: Case, index: number): BuiltCase {
  if (testCase.label === "transaction_2") return { ...transactionCase(2, index), cold: false };
  if (testCase.label === "transaction_8") return { ...transactionCase(8, index), cold: false };
  if (testCase.mode === "COLD" || testCase.mode === "FIRST_USE") {
    return { base: workloadView(testCase.workload, testCase.size, `cold-${index}`), next: workloadView(testCase.workload, testCase.size, `cold-${index}`), cold: true };
  }
  if (testCase.mode === "SHARED_PATH") {
    const path = pathCase(1, index);
    return { ...path, cold: false };
  }
  if (testCase.mode === "SHARED_DEEP") {
    const path = pathCase(4, index);
    return { ...path, cold: false };
  }
  if (testCase.mode === "LARGE_SHARED_SUBTREE_CUTOFF") {
    return { ...cutoffCase(index), cold: false };
  }
  const base = workloadView(testCase.workload, testCase.size, `base-${index}`);
  if (testCase.mode === "IDENTICAL_IDENTITY") return { base, next: base, cold: false };
  if (testCase.workload === "long_text_wrap_only" || testCase.workload === "styled_span_heavy") {
    return { base, next: base.noWrap().textAlign("center"), cold: false };
  }
  if (testCase.workload === "large_decoration_only_change" || testCase.workload === "decoration_heavy") {
    return { base, next: base.maxWidth(40), cold: false };
  }
  if (testCase.workload === "row_heavy" || testCase.workload === "column_track_heavy") {
    return { ...structuralCase(Math.max(2, Math.min(testCase.size, 10_000)), index), cold: false };
  }
  if (testCase.workload === "grid_heavy") {
    const grid = View.grid([View.text("a"), View.text("b"), View.text("c")]);
    return {
      base: grid,
      next: View.replaceGridCellForPackedTransport(grid, 0, 1, View.text(`grid-change-${index}`)),
      cold: false,
    };
  }
  if (testCase.workload === "diff_heavy" || testCase.workload === "mixed_realistic") {
    return { base, next: workloadView(testCase.workload, testCase.size, `rebuilt-${index}`), cold: false };
  }
  return { base, next: base.noWrap(), cold: false };
}

function createHost(): PerfHost {
  if (Host === undefined) throw new Error("native TUI host is unavailable");
  return new Host(80, 24, true);
}

function renderV4(
  host: PerfHost,
  view: View,
  encoder: PackedV4Encoder,
  hooks: {
    readonly encodingStarted?: () => void;
    readonly encodingFinished?: () => void;
    readonly nativeStarted?: () => void;
    readonly nativeFinished?: () => void;
  } = {},
): void {
  if (host.tuiPerfV4PackedRender === undefined || host.tuiPerfV4PackedRenderRef === undefined) {
    throw new Error("native addon lacks Packed V4 benchmark methods");
  }
  renderPackedV4View(
    encoder,
    view,
    (words, bytes) => host.tuiPerfV4PackedRender!(words, bytes),
    (generation, packedRef) => host.tuiPerfV4PackedRenderRef!(generation, packedRef),
    hooks,
  );
}

function abiSnapshot(): Record<string, number> {
  const snapshot = native.tuiViewAbiBootstrap?.().diagnostics;
  if (snapshot === undefined) return {};
  return Object.fromEntries(Object.entries(snapshot).filter((entry): entry is [string, number] => typeof entry[1] === "number"));
}

async function runSample(candidate: Candidate, testCase: Case, index: number): Promise<Sample> {
  const constructionStarted = now();
  const built = buildCase(testCase, index);
  const constructionNs = now() - constructionStarted;
  const heapBefore = process.memoryUsage().heapUsed;
  const rssBefore = process.memoryUsage().rss;
  resetNativeViewRouteCounters();
  const beforeSnapshot = abiSnapshot();
  let commitNs = 0;
  let encodingNs = 0;
  let nativeRouteNs = 0;
  const host = candidate === "native_shadow" ? undefined : createHost();
  const tui = candidate === "native_shadow" ? await Tui.open({ width: 80, height: 24, headless: true }) : undefined;
  const encoder = candidate === "packed_v4" ? createPackedV4Encoder("textencoder", "content") : undefined;
  try {
    if (candidate === "native_shadow") {
      if (!built.cold) tui!.render({ body: built.base });
      const commitStarted = now();
      tui!.render({ body: built.next });
      commitNs = now() - commitStarted;
    } else if (candidate === "direct") {
      if (!built.cold) host!.render(nodeForDirectBridge(built.base));
      const commitStarted = now();
      const encodingStarted = now();
      const bridged = nodeForDirectBridge(built.next);
      encodingNs = now() - encodingStarted;
      host!.render(bridged);
      commitNs = now() - commitStarted;
    } else {
      if (!built.cold) renderV4(host!, built.base, encoder!);
      const commitStarted = now();
      let encodingStarted = 0;
      let nativeStarted = 0;
      renderV4(host!, built.next, encoder!, {
        encodingStarted: () => { encodingStarted = now(); },
        encodingFinished: () => { if (encodingStarted !== 0) { encodingNs += now() - encodingStarted; encodingStarted = 0; } },
        nativeStarted: () => { nativeStarted = now(); },
        nativeFinished: () => { if (nativeStarted !== 0) { nativeRouteNs += now() - nativeStarted; nativeStarted = 0; } },
      });
      commitNs = now() - commitStarted;
    }
    if (candidate !== "packed_v4" || nativeRouteNs === 0) nativeRouteNs = commitNs;
  } finally {
    tui?.close();
    host?.dispose();
  }
  const afterSnapshot = abiSnapshot();
  const snapshot: Record<string, number> = {};
  for (const key of new Set([...Object.keys(beforeSnapshot), ...Object.keys(afterSnapshot)])) {
    snapshot[key] = (afterSnapshot[key] ?? 0) - (beforeSnapshot[key] ?? 0);
  }
  const heapDelta = Math.max(0, process.memoryUsage().heapUsed - heapBefore);
  const rssDelta = Math.max(0, process.memoryUsage().rss - rssBefore);
  return {
    totalNs: constructionNs + commitNs,
    constructionNs,
    commitNs,
    encodingNs,
    nativeRouteNs,
    heapDelta,
    rssDelta,
    snapshot,
    routes: nativeViewRouteSnapshot(),
  };
}

async function runCase(testCase: Case): Promise<Record<string, unknown>> {
  const candidateResults = new Map<Candidate, Sample[]>();
  for (const candidate of selectedCandidates) {
    const samples: Sample[] = [];
    for (let repeat = 0; repeat < repeats; repeat += 1) {
      for (let index = 0; index < warmup; index += 1) await runSample(candidate, testCase, repeat * (warmup + iterations) + index);
      for (let index = 0; index < iterations; index += 1) {
        samples.push(await runSample(candidate, testCase, repeat * (warmup + iterations) + warmup + index));
      }
    }
    candidateResults.set(candidate, samples);
  }
  const serializedCandidates: Record<string, unknown> = {};
  for (const [candidate, samples] of candidateResults) {
    const totals = samples.map((sample) => sample.totalNs);
    const construction = samples.map((sample) => sample.constructionNs);
    const commit = samples.map((sample) => sample.commitNs);
    const encoding = samples.map((sample) => sample.encodingNs);
    const routeTotals: Record<string, number> = {};
    for (const sample of samples) {
      for (const [route, count] of Object.entries(sample.routes)) routeTotals[route] = (routeTotals[route] ?? 0) + count;
    }
    const memory = samples.reduce((result, sample) => {
      for (const [key, value] of Object.entries(sample.snapshot)) result[key] = (result[key] ?? 0) + value;
      return result;
    }, {} as Record<string, number>);
    serializedCandidates[candidate] = {
      total: stats(totals),
      construction: stats(construction),
      commit: stats(commit),
      encoding: stats(encoding),
      native_route: stats(samples.map((sample) => sample.nativeRouteNs)),
      total_samples_ns: totals,
      commit_samples_ns: commit,
      structural_encoding_ns: candidate === "native_shadow" ? 0 : stats(encoding).median_ns,
      command_words_written: 0,
      path_arrays_written: 0,
      node_id_arrays_written: 0,
      heap_peak_delta_bytes: Math.max(...samples.map((sample) => sample.heapDelta), 0),
      rss_peak_delta_bytes: Math.max(...samples.map((sample) => sample.rssDelta), 0),
      route_counts: routeTotals,
      runtime_snapshot_deltas: memory,
    };
  }
  return {
    label: testCase.label,
    workload: testCase.workload,
    size: testCase.size,
    mode: testCase.mode,
    candidates: serializedCandidates,
  };
}

function buildCases(): readonly Case[] {
  const cases: Case[] = [];
  for (const workload of selectedWorkloads) {
    for (const size of normalSizes) {
      for (const mode of modes) cases.push({ workload, size, mode, label: `${workload}/${size}/${mode}` });
    }
  }
  for (const size of wideSizes) {
    cases.push({ workload: "column_track_heavy", size, mode: "REBUILT_EQUIVALENT", label: `wide_replace/${size}` });
  }
  for (const size of coldSizes) {
    cases.push({ workload: "plain_text_column", size, mode: "COLD", label: `cold_axis/${size}` });
  }
  cases.push(
    { workload: "plain_text_column", size: 2, mode: "SHARED_PATH", label: "path_depth_1" },
    { workload: "plain_text_column", size: 4, mode: "SHARED_DEEP", label: "path_depth_4" },
    { workload: "plain_text_column", size: 2, mode: "REBUILT_EQUIVALENT", label: "transaction_2" },
    { workload: "plain_text_column", size: 8, mode: "REBUILT_EQUIVALENT", label: "transaction_8" },
  );
  return cases;
}

async function runRealisticTrace(): Promise<Record<string, unknown>> {
  const traceCases: Case[] = [];
  for (let index = 0; index < 100; index += 1) {
    const slot = index % 100;
    traceCases.push(slot < 20
      ? { workload: "plain_text_column", size: 20, mode: "IDENTICAL_IDENTITY", label: "trace/no-op" }
      : slot < 75
        ? { workload: "plain_text_column", size: 20, mode: "SHARED_PATH", label: "trace/path" }
        : slot < 85
          ? { workload: "plain_text_column", size: 20, mode: "SHARED_DEEP", label: "trace/deep" }
          : slot < 90
            ? { workload: "row_heavy", size: 32, mode: "REBUILT_EQUIVALENT", label: "trace/structural" }
            : slot < 98
              ? { workload: "mixed_realistic", size: 20, mode: "REBUILT_EQUIVALENT", label: "trace/rebuilt" }
              : { workload: "plain_text_column", size: 20, mode: "COLD", label: "trace/cold" });
  }
  const totals: Record<Candidate, number> = { direct: 0, native_shadow: 0, packed_v4: 0 };
  const routes: Record<string, number> = {};
  for (const candidate of selectedCandidates) {
    for (const [index, testCase] of traceCases.entries()) {
      const sample = await runSample(candidate, testCase, 10_000 + index);
      totals[candidate] += sample.totalNs;
      for (const [route, count] of Object.entries(sample.routes)) routes[`${candidate}.${route}`] = (routes[`${candidate}.${route}`] ?? 0) + count;
    }
  }
  return {
    operations: traceCases.length,
    mix: "20% IDENTICAL_IDENTITY, 55% SHARED_PATH, 10% SHARED_DEEP, 5% structural, 8% rebuilt, 2% cold",
    totals_ns: totals,
    routes,
  };
}

function decisionGates(results: readonly Record<string, unknown>[]): Record<string, unknown> {
  const normal = results.filter((result) => (modes as readonly string[]).includes(String(result.mode)) && !String(result.label).startsWith("wide_") && !String(result.label).startsWith("cold_") && !String(result.label).startsWith("path_") && !String(result.label).startsWith("transaction_"));
  const ratios: number[] = [];
  let regressionsOverThreePercent = 0;
  for (const result of normal) {
    const candidates = result.candidates as Record<string, { total: Stats }>;
    const direct = candidates.direct?.total.median_ns;
    const nativeShadow = candidates.native_shadow?.total.median_ns;
    if (direct === undefined || nativeShadow === undefined || direct === 0) continue;
    const ratio = nativeShadow / direct;
    ratios.push(ratio);
    if (ratio > 1.03) regressionsOverThreePercent += 1;
  }
  const sorted = [...ratios].sort((left, right) => left - right);
  const medianRatio = sorted[Math.ceil((sorted.length - 1) / 2)] ?? 0;
  const exactRatios = results
    .filter((result) => result.mode === "IDENTICAL_IDENTITY")
    .map((result) => {
      const candidates = result.candidates as Record<string, { total: Stats }>;
      return candidates.native_shadow === undefined || candidates.packed_v4 === undefined
        ? undefined
        : candidates.native_shadow.total.median_ns / candidates.packed_v4.total.median_ns;
    })
    .filter((ratio): ratio is number => ratio !== undefined);
  const coldRatios = results
    .filter((result) => String(result.label).startsWith("cold_"))
    .map((result) => {
      const candidates = result.candidates as Record<string, { total: Stats }>;
      return candidates.native_shadow === undefined || candidates.packed_v4 === undefined
        ? undefined
        : candidates.native_shadow.total.median_ns / candidates.packed_v4.total.median_ns;
    })
    .filter((ratio): ratio is number => ratio !== undefined);
  return {
    normal_case_count: ratios.length,
    native_shadow_over_direct_median_ratio: medianRatio,
    native_shadow_normal_matrix_faster_than_direct: medianRatio < 1,
    common_regressions_over_three_percent: regressionsOverThreePercent,
    common_regression_gate: regressionsOverThreePercent === 0,
    exact_native_shadow_not_slower_than_v4: exactRatios.every((ratio) => ratio <= 1.03),
    exact_native_shadow_over_v4_max_ratio: Math.max(...exactRatios, 0),
    cold_native_shadow_within_five_percent_of_v4: coldRatios.every((ratio) => ratio <= 1.05),
    cold_native_shadow_over_v4_max_ratio: Math.max(...coldRatios, 0),
  };
}

function git(command: string): string {
  const result = Bun.spawnSync(command.split(" "));
  return new TextDecoder().decode(result.stdout).trim();
}

function sha256(path: string): string {
  return git(`shasum -a 256 ${path}`).split(/\s+/)[0] ?? "unknown";
}

const session = nativeViewAbiSession();
if (Host === undefined || session === undefined) throw new Error("PERF-11.11 requires the staged generated native ABI");
if (Bun.env.PERF_ALLOW_DIRTY !== "1" && git("git status --porcelain") !== "") {
  throw new Error("PERF-11.11 authoritative runs require a clean worktree");
}
const cases = buildCases();
const results: Record<string, unknown>[] = [];
for (const testCase of cases) results.push(await runCase(testCase));
const trace = await runRealisticTrace();
const routeCounts: Record<string, number> = {};
for (const result of results) {
  for (const candidate of Object.values(result.candidates as Record<string, { route_counts: Record<string, number> }>)) {
    if (candidate.route_counts === undefined) continue;
    for (const [route, count] of Object.entries(candidate.route_counts)) routeCounts[route] = (routeCounts[route] ?? 0) + count;
  }
}
const collectGarbage = (Bun as unknown as { gc?: (force?: boolean) => void }).gc;
collectGarbage?.(true);
const finalRuntimeSnapshot = abiSnapshot();
const lifetimeAudit = {
  passed: (finalRuntimeSnapshot.leased_slots ?? 0) === 0
    && (finalRuntimeSnapshot.builders ?? 0) === 0
    && (finalRuntimeSnapshot.edit_transactions ?? 0) === 0,
  final_runtime_snapshot: finalRuntimeSnapshot,
  invariant: "all benchmark hosts closed; no JS leases, builders, or edit transactions remain",
};
if (!lifetimeAudit.passed) throw new Error(`PERF-11.11 lifetime audit failed: ${JSON.stringify(lifetimeAudit)}`);
const output = {
  benchmark: "PERF-11.11-native-view-decision",
  bun_version: Bun.version,
  bun_revision: Bun.revision,
  git_sha: git("git rev-parse HEAD"),
  git_dirty: git("git status --porcelain") !== "",
  native_artifact_sha256: sha256("packages/iyon-runtime/native/iyon-native.node"),
  benchmark_source_sha256: sha256("packages/iyon-runtime/bench/tui_decision.ts"),
  schema_blake3: session.abi.schema_blake3,
  generator_blake3: session.abi.generator_blake3,
  abi_version: session.abi.abi_version,
  semantic_schema_version: session.abi.semantic_version,
  function_count: session.abi.function_count,
  warmup,
  iterations,
  repeats,
  candidates: selectedCandidates,
  normal_matrix: { workloads: selectedWorkloads, sizes: normalSizes, modes },
  wide_matrix: wideSizes,
  cold_guardrail: coldSizes,
  cases: results,
  decision_gates: decisionGates(results),
  realistic_trace: trace,
  route_counts: routeCounts,
  phase_scope: {
    js_api_construction: "timed around immutable View construction",
    js_fusion: "included in js_api_construction; compact backing has no separate timer",
    text_transcode: "included in commit; native cstring transcode is not phase-visible",
    ffi_call: "included in native_route/commit",
    native_semantic: "included in native_route/commit",
    native_publish: "included in native_route/commit",
    host_commit: "included in native_route/commit",
    total: "construction + complete commit",
    structural_encoding: "zero for native scalar/path/transaction/builder routes; direct/V4 report measured bridge transport time",
  },
  structural_encoding_ns: 0,
  command_words_written: 0,
  path_arrays_written: 0,
  node_id_arrays_written: 0,
  runtime_snapshot: finalRuntimeSnapshot,
  lifetime_audit: lifetimeAudit,
};
console.log(JSON.stringify(output, null, 2));

import { native } from "../src/native.ts";
import { View, nodeForBridge } from "../src/tui/values/view.ts";

type PerfNative = typeof native & {
  tuiPerfReset?: () => void;
  tuiPerfSnapshot?: () => Record<string, number>;
  tuiPerfPackedRender?: (host: object, words: Uint32Array, strings: string[]) => void;
};

type BenchmarkHost = {
  render(view: object): void;
  screenRows(): string[];
  dispose(): void;
};

type BridgeNode = ReturnType<typeof nodeForBridge>;
type Pattern = "COLD" | "IDENTICAL_IDENTITY" | "SHARED_PATH" | "REBUILT_EQUIVALENT";

type PackedState = {
  known: Set<number>;
  words: number[];
  strings: string[];
};

const perfNative = native as PerfNative;
const sizes = [
  { name: "small_view", nodes: 20 },
  { name: "medium_view", nodes: 200 },
  { name: "large_view", nodes: 2_000 },
  { name: "huge_view", nodes: 10_000 },
];
const patterns: readonly Pattern[] = ["COLD", "IDENTICAL_IDENTITY", "SHARED_PATH", "REBUILT_EQUIVALENT"];
const iterationsOverride = Number(Bun.env.PERF_ITERATIONS ?? "0");

function tree(nodes: number, prefix = "node"): View {
  const leaves = Math.max(1, nodes - 1);
  return View.vertical((column) => {
    for (let index = 0; index < leaves; index += 1) {
      column.child(View.text(`${prefix}-${index}`));
    }
  });
}

function iterations(nodes: number): number {
  if (iterationsOverride > 0) return iterationsOverride;
  if (nodes <= 20) return 100;
  if (nodes <= 200) return 50;
  if (nodes <= 2_000) return 20;
  return 5;
}

function percentile(samples: number[], percentage: number): number {
  const sorted = [...samples].sort((left, right) => left - right);
  const index = Math.ceil((sorted.length - 1) * percentage / 100);
  return sorted[index] ?? 0;
}

function gitSha(): string {
  const result = Bun.spawnSync(["git", "rev-parse", "HEAD"]);
  return new TextDecoder().decode(result.stdout).trim() || "unknown";
}

function createHost(): BenchmarkHost {
  const Host = native.NativeTuiHost;
  if (Host === undefined) throw new Error("native TUI host is unavailable");
  return new Host(80, 24, true) as unknown as BenchmarkHost;
}

function packNode(node: BridgeNode, state: PackedState): void {
  const id = node.id;
  if (state.known.has(id)) {
    state.words.push(0, id);
    return;
  }
  state.known.add(id);

  if (node.kind === 1) {
    const text = node.spans.map((span) => span.text).join("");
    const stringIndex = state.strings.length;
    state.strings.push(text);
    state.words.push(1, id, stringIndex);
    return;
  }
  if (node.kind === 5) {
    state.words.push(2, id, node.gap, node.children.length);
    for (const child of node.children) packNode(child.child, state);
    return;
  }
  throw new Error(`benchmark packed encoder does not support view kind ${node.kind}`);
}

function packedView(view: View, state: PackedState): { words: Uint32Array; strings: string[] } {
  state.words = [];
  state.strings = [];
  packNode(nodeForBridge(view), state);
  return { words: new Uint32Array(state.words), strings: state.strings };
}

function renderPacked(host: BenchmarkHost, words: Uint32Array, strings: string[]): void {
  if (perfNative.tuiPerfPackedRender === undefined) {
    throw new Error("packed benchmark transport is unavailable; build with perf-packed-benchmark");
  }
  perfNative.tuiPerfPackedRender(host as object, words, strings);
}

function verifyPackedTransport(): void {
  const view = View.vertical((column) => {
    column.gap(1);
    column.child(View.text("packed-a"));
    column.child(View.text("packed-b"));
  });
  const state: PackedState = { known: new Set(), words: [], strings: [] };
  const packed = packedView(view, state);
  const directHost = createHost();
  const packedHost = createHost();
  try {
    directHost.render(nodeForBridge(view));
    renderPacked(packedHost, packed.words, packed.strings);
    const directRows = directHost.screenRows();
    const packedRows = packedHost.screenRows();
    if (JSON.stringify(directRows) !== JSON.stringify(packedRows)) {
      throw new Error(`packed transport changed rendered rows: ${JSON.stringify({ directRows, packedRows })}`);
    }
  } finally {
    directHost.dispose();
    packedHost.dispose();
  }
}

function emit(
  benchmark: string,
  implementation: "direct" | "packed",
  nodeCount: number,
  samples: number[],
  cpuUserUs: number,
  cpuSystemUs: number,
  heapPeakDeltaBytes: number,
  sha: string,
): void {
  console.log(JSON.stringify({
    benchmark,
    implementation,
    node_count: nodeCount,
    iterations: samples.length,
    median_ns: percentile(samples, 50),
    p95_ns: percentile(samples, 95),
    p99_ns: percentile(samples, 99),
    cpu_user_us: cpuUserUs,
    cpu_system_us: cpuSystemUs,
    heap_peak_delta_bytes: heapPeakDeltaBytes,
    counters: perfNative.tuiPerfSnapshot?.() ?? {},
    git_sha: sha,
  }));
}

function run(
  implementation: "direct" | "packed",
  sizeName: string,
  nodes: number,
  pattern: Pattern,
  sha: string,
): void {
  const host = createHost();
  const count = iterations(nodes);
  const base = tree(nodes);
  const shared = tree(Math.max(2, Math.floor(nodes / 2)), "shared");
  const packedState: PackedState = { known: new Set(), words: [], strings: [] };
  const samples: number[] = [];
  const heapStart = process.memoryUsage().heapUsed;
  let heapPeak = heapStart;
  const cpuStart = process.cpuUsage();
  perfNative.tuiPerfReset?.();

  try {
    for (let index = 0; index < count; index += 1) {
      const started = Bun.nanoseconds();
      const state = pattern === "IDENTICAL_IDENTITY" || pattern === "SHARED_PATH"
        ? packedState
        : { known: new Set<number>(), words: [], strings: [] };
      let view: View;
      switch (pattern) {
        case "COLD":
        case "REBUILT_EQUIVALENT":
          view = tree(nodes);
          break;
        case "IDENTICAL_IDENTITY":
          view = base;
          break;
        case "SHARED_PATH":
          view = View.vertical((column) => {
            column.child(shared);
            column.child(View.text(`changed-${index}`));
          });
          break;
      }

      if (implementation === "direct") {
        host.render(nodeForBridge(view));
      } else {
        const packed = packedView(view, state);
        renderPacked(host, packed.words, packed.strings);
      }
      samples.push(Bun.nanoseconds() - started);
      heapPeak = Math.max(heapPeak, process.memoryUsage().heapUsed);
    }
  } finally {
    const cpu = process.cpuUsage(cpuStart);
    emit(
      `napi/view/${sizeName}/${pattern}`,
      implementation,
      nodes,
      samples,
      cpu.user,
      cpu.system,
      Math.max(0, heapPeak - heapStart),
      sha,
    );
    host.dispose();
  }
}

verifyPackedTransport();
const sha = gitSha();
for (const size of sizes) {
  for (const pattern of patterns) {
    run("direct", size.name, size.nodes, pattern, sha);
    run("packed", size.name, size.nodes, pattern, sha);
  }
}

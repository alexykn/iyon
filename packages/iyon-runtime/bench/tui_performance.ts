import { native } from "../src/native.ts";
import { View, nodeForBridge } from "../src/tui/values/view.ts";

type PerfNative = typeof native & {
  tuiPerfReset?: () => void;
  tuiPerfSnapshot?: () => Record<string, number>;
};

const perfNative = native as PerfNative;
const sizes = [
  { name: "small_view", nodes: 20 },
  { name: "medium_view", nodes: 200 },
  { name: "large_view", nodes: 2_000 },
  { name: "huge_view", nodes: 10_000 },
];
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

function emit(
  benchmark: string,
  nodeCount: number,
  samples: number[],
  sha: string,
): void {
  console.log(JSON.stringify({
    benchmark,
    implementation: "baseline",
    node_count: nodeCount,
    source_bytes: 0,
    iterations: samples.length,
    median_ns: percentile(samples, 50),
    p95_ns: percentile(samples, 95),
    p99_ns: percentile(samples, 99),
    counters: perfNative.tuiPerfSnapshot?.() ?? {},
    git_sha: sha,
  }));
}

const benchmarkHost = (() => {
  const Host = native.NativeTuiHost;
  if (Host === undefined) throw new Error("native TUI host is unavailable");
  return new Host(80, 24, true);
})();

function render(view: View): void {
  benchmarkHost.render(nodeForBridge(view));
}

function run(sizeName: string, nodes: number, pattern: "COLD" | "IDENTICAL_IDENTITY" | "SHARED_PATH" | "REBUILT_EQUIVALENT", sha: string): void {
  const count = iterations(nodes);
  const base = tree(nodes);
  const shared = tree(Math.max(2, Math.floor(nodes / 2)), "shared");
  const samples: number[] = [];
  perfNative.tuiPerfReset?.();

  for (let index = 0; index < count; index += 1) {
    const started = Bun.nanoseconds();
    switch (pattern) {
      case "COLD":
        render(tree(nodes));
        break;
      case "IDENTICAL_IDENTITY":
        render(base);
        break;
      case "SHARED_PATH":
        render(View.vertical((column) => {
          column.child(shared);
          column.child(View.text(`changed-${index}`));
        }));
        break;
      case "REBUILT_EQUIVALENT":
        render(tree(nodes));
        break;
    }
    samples.push(Bun.nanoseconds() - started);
  }

  emit(`napi/view/${sizeName}/${pattern}`, nodes, samples, sha);
}

const sha = gitSha();
for (const size of sizes) {
  for (const pattern of ["COLD", "IDENTICAL_IDENTITY", "SHARED_PATH", "REBUILT_EQUIVALENT"] as const) {
    run(size.name, size.nodes, pattern, sha);
  }
}

benchmarkHost.dispose();

import { createHash } from "node:crypto";
import { native } from "../src/native.ts";
import {
  viewAxisSetChild,
  viewAxisSpliceBuffer,
} from "../src/tui/generated/view_calls.ts";
import {
  nativeViewAbiSession,
  nativeViewRefForNodeId,
  releaseNativeViewRef,
} from "../src/tui/native_view_abi.ts";
import {
  nodeForBridge,
  nodeIdPair,
  replaceAxisChildForPackedTransport,
  spliceAxisChildrenForPackedTransport,
  View,
} from "../src/tui/values/view.ts";
import manifest from "../src/tui/generated/view_abi_manifest.json";

const WIDTHS = [2_048, 10_000, 100_000] as const;
const WARMUP = 50;
const ITERATIONS = 100;
const REPEATS = 5;
const Host = native.NativeTuiHost as unknown as (new (width: number, height: number, headless: boolean) => {
  render(view: object): void;
  tuiViewAbiHostPointer(): number;
  dispose(): void;
}) | undefined;

type Prepared = {
  readonly session: NonNullable<ReturnType<typeof nativeViewAbiSession>>;
  readonly host: InstanceType<NonNullable<typeof Host>>;
  readonly baseRef: number;
  readonly width: number;
  readonly index: number;
  readonly base: View;
  readonly replacement: View;
  readonly replacementRef: number;
  readonly nodeIds: Readonly<Record<"replace" | "insert" | "remove", readonly (readonly [number, number])[]>>;
};

function median(values: readonly number[]): number {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.floor(sorted.length / 2)]!;
}

function invoke(
  prepared: Prepared,
  operation: "replace" | "insert" | "remove",
  iteration: number,
): number {
  const { session, baseRef, replacementRef } = prepared;
  // Each sample uses a distinct real JS semantic identity. The corresponding
  // immutable roots are prebuilt outside the timed native operation, so native
  // publication never compares an already-published wide root.
  const [low, high] = prepared.nodeIds[operation][iteration]!;
  if (operation === "replace") {
    return viewAxisSetChild(session.symbols, session.runtime, baseRef, low, high, prepared.index, 0, replacementRef);
  }
  const values = operation === "insert" ? new Uint32Array([0, replacementRef]) : new Uint32Array(2);
  return viewAxisSpliceBuffer(
    session.symbols,
    session.runtime,
    baseRef,
    low,
    high,
    prepared.index,
    operation === "insert" ? 0 : 1,
    values,
    operation === "insert" ? 1 : 0,
  );
}

function measure(prepared: Prepared, operation: "replace" | "insert" | "remove"): number {
  let checksum = 0;
  for (let iteration = 0; iteration < WARMUP; iteration += 1) invoke(prepared, operation, iteration);
  const samples: number[] = [];
  for (let repeat = 0; repeat < REPEATS; repeat += 1) {
    const start = Bun.nanoseconds();
    for (let iteration = 0; iteration < ITERATIONS; iteration += 1) {
      const ref = invoke(prepared, operation, WARMUP + repeat * ITERATIONS + iteration);
      checksum = (checksum + ref) >>> 0;
    }
    samples.push(Number(Bun.nanoseconds() - start) / ITERATIONS);
  }
  if (checksum === 0) throw new Error(`${operation} benchmark checksum unexpectedly zero`);
  return median(samples);
}

function prepare(width: number, session: Prepared["session"]): Prepared {
  if (Host === undefined) throw new Error("NativeTuiHost is unavailable");
  const host = new Host(80, Math.min(width + 2, 2_050), true);
  const baseChildren = Array.from({ length: width }, (_, index) => View.text(`item-${index}`));
  const base = View.vertical(baseChildren);
  const replacement = View.text("replacement");
  host.render(nodeForBridge(replacement));
  const replacementRef = nativeViewRefForNodeId(replacement);
  if (replacementRef === undefined) throw new Error("replacement ref unavailable");
  host.render(nodeForBridge(base));
  const baseRef = nativeViewRefForNodeId(base);
  if (baseRef === undefined) throw new Error("base ref unavailable");
  const totalSamples = WARMUP + REPEATS * ITERATIONS;
  const nodeIds = {
    replace: Array.from({ length: totalSamples }, () => nodeIdPair(replaceAxisChildForPackedTransport(base, 1_000, replacement))),
    insert: Array.from({ length: totalSamples }, () => nodeIdPair(spliceAxisChildrenForPackedTransport(base, 1_000, 0, [replacement]))),
    remove: Array.from({ length: totalSamples }, () => nodeIdPair(spliceAxisChildrenForPackedTransport(base, 1_000, 1, []))),
  } as const;
  return {
    session,
    host,
    baseRef,
    width,
    index: 1_000,
    base,
    replacement,
    replacementRef,
    nodeIds,
  };
}

const session = nativeViewAbiSession();
if (session === undefined) throw new Error("generated native View ABI is unavailable");
if (Host === undefined) throw new Error("NativeTuiHost is unavailable");
const results: Record<string, Record<string, number>> = {};
for (const width of WIDTHS) {
  const prepared = prepare(width, session);
  try {
    const replace = measure(prepared, "replace");
    const insert = measure(prepared, "insert");
    const remove = measure(prepared, "remove");
    results[String(width)] = {
      replace_median_ns_per_render: replace,
      insert_median_ns_per_render: insert,
      remove_median_ns_per_render: remove,
    };
  } finally {
    releaseNativeViewRef(session, prepared.replacementRef);
    releaseNativeViewRef(session, prepared.baseRef);
    prepared.host.dispose();
  }
}

const nativeArtifact = await Bun.file(new URL("../native/iyon-native.node", import.meta.url)).arrayBuffer();
const nativeArtifactSha256 = createHash("sha256").update(new Uint8Array(nativeArtifact)).digest("hex");
const git = (command: string): string => {
  const result = Bun.spawnSync(["git", ...command.split(" ")], { stdout: "pipe", stderr: "ignore" });
  return new TextDecoder().decode(result.stdout).trim();
};
console.log(JSON.stringify({
  benchmark: "PERF-11.7-native-persistent-seq",
  bun_version: Bun.version,
  bun_revision: Bun.revision,
  git_sha: git("rev-parse HEAD"),
  git_dirty: git("status --porcelain") !== "",
  native_artifact_sha256: nativeArtifactSha256,
  schema_blake3: manifest.schema_blake3,
  generator_blake3: manifest.generator_blake3,
  abi_version: session.abi.abi_version,
  semantic_schema_version: session.abi.semantic_version,
  function_count: session.abi.function_count,
  widths: WIDTHS,
  warmup: WARMUP,
  iterations: ITERATIONS,
  repeats: REPEATS,
  route: "typed axis structural call -> native PersistentSeq path-copy (host parity separately tested)",
  structural_encoding_ns: 0,
  command_words_written: 0,
  path_arrays_written: 0,
  node_id_arrays_written: 0,
  persistent_seq_flatten_calls: 0,
  results,
}, null, 2));

# Tranche 11 — TUI bridge optimization

## Mission

After T10 has established functional parity, measure the Bun/TypeScript `iyon:tui` path against a Rust baseline and reduce only the bridge overhead that profiling proves material. Keep the public TypeScript `View` API and the native `iyon-tui` layout, projection, terminal, History, and streaming semantics unchanged; the intended result is fewer N-API crossings and fewer temporary allocations while the native engine continues to own correctness-critical state.

## Prerequisites

T0–T2 must have frozen the architecture and API-surface dispositions; T1 must provide the buildable `crates/iyon-native` N-API addon and Bun runtime; T5 must provide the complete, tested `iyon:tui` TypeScript surface, including lazy `View` values and native handles for `History`, `TextInput`, and streams; T6–T9 must have the common extension/runtime path; and T10 must have the parity Bun app and CLI using that path. The Rust workspace and the Bun/TypeScript path must be green before this tranche starts.

T11 may rely on the T5/T10 module locations and internal adapters, but it must not assume T12’s packaging, stress, ecosystem, or release automation. A benchmark fixture may call the public `iyon:tui` surface and the native test hooks introduced here; it must not call product-private Rust application functions.

## Invariants (do not violate)

- Bun remains the process owner. `iyon-native` is a thin bridge, not a product or scheduling layer; it must not regain a Rust `main`, embed Bun, or own application policy.
- The Rust dependency graph remains `iyon-api → iyon-core`, with `iyon-tui` independent of both and `iyon-native` depending on the three native crates. No optimization may introduce `iyon-core` or `iyon-api` into `iyon-tui`.
- The public TypeScript API remains unchanged. In particular, `View.text("hello").bold().padding(1)` and all existing `iyon:tui` imports keep their signatures and behavior. Command buffers, intern tables, batching, and pools are internal implementation details.
- `View` remains a lazy semantic value until native materialization. Do not generate one N-API call per fluent builder operation, and do not duplicate layout, projection, wrapping, painting, or terminal logic in TypeScript.
- Stateful native objects remain native handles. Do not turn `History`, scrollback, `TextInput`, or stream state into JSON snapshots or recreate them in TypeScript.
- Native calls own their data across async boundaries, validate all indices/opcodes, return typed errors, and provide explicit cancellation for any long-lived operation. No borrowed N-API value may survive a suspended Rust future.
- Native scrollback and promotion correctness are frozen. A performance change must preserve unit order, flow boundaries, stable/unstable stream prefixes, terminal acknowledgements, viewport anchoring, and the existing native history frontier.
- Measure before optimizing. The committed benchmark manifest records hardware/runtime/build metadata, workload parameters, phase timings, N-API crossing counts, allocation/copy counters where available, and an explicit acceptance threshold. Results are not accepted from a synthetic View-only benchmark.
- The explicit T11 target is p95 TS-app overhead no greater than 1.25× the Rust baseline for each measured phase and workload after warm-up, with no dropped or reordered stream updates at 10/50/200 updates per second. Any exception must be recorded in the benchmark report with a user-visible rationale; it is not a silent threshold relaxation.

## Out of scope

- No public TypeScript or Rust API redesign, new product feature, new extension contribution type, or change to extension activation/loader behavior.
- No rewrite of native scrollback, History promotion rules, layout, projection algebra, terminal backends, or the Rust-owned rendering engine. T11 may add a bridge entry point that invokes those existing operations in batches, but not a second implementation of them.
- No WASM, Rhai, WIT, embedded/custom JavaScript engine, Rust-owned subprocess, Rust HTTP/provider layer, or special built-in/native application path.
- No migration of providers, tools, the default agent, or the default app; those belong to T7–T10 and must remain ordinary TypeScript consumers of the optimized API.
- No broad dependency update or new benchmark framework merely for convenience. Use existing Rust/Bun tooling and standard timing/serialization facilities unless a concrete blocker is documented.
- No T12 packaging, standalone executable release work, ecosystem documentation, stress suite, or release CI. T11 leaves reusable benchmark commands and fixtures for T12; T12 decides how they enter the release pipeline.

## Adjacent tranche contracts

### Consumes from earlier tranches

- T1 exports a buildable native addon and runtime loading path.
- T2 exports the API-surface inventory and the rule that a lazy TypeScript façade is a complete binding; T11 must update any internal disposition/benchmark metadata needed to keep that inventory truthful, without creating a one-symbol-per-method ABI.
- T5 exports the parity `iyon:tui` façades and native-handle ownership model. T11 consumes their internal bridge adapter rather than changing the public façade.
- T6–T10 export the common extension loader and the Bun-owned app path. The benchmark must exercise the same runtime path used by bundled and third-party extensions, not a privileged benchmark-only loader.

### Exports for later tranches

- A compact, versioned internal View command-buffer format and one native materialization operation, with validation and differential tests.
- Batch-capable internal bridge operations for stream updates, History promotion, and (only where profiling justifies it) repeated TextInput activity.
- Reproducible Rust-vs-TS fixtures, phase/crossing telemetry, baseline and final result schemas, and a local threshold-check command that T12 can reuse for stress and packaging verification.
- A benchmark report that names the accepted overhead, workload matrix, environment, and any deliberately unoptimized path retained for correctness.

### Must not steal from neighbors

- Do not make T12’s stress, release, package embedding, or ecosystem decisions in this tranche; expose stable benchmark inputs and commands for it instead.
- Do not change T10’s user-visible app behavior or CLI ownership; only replace the internal bridge implementation behind the existing app path.
- Do not move provider, tool, agent, or app policy into `iyon-native`, and do not add a native shortcut unavailable to ordinary TypeScript extensions.

## Commits

### Commit 1 — Add the reproducible TUI benchmark harness

**Why:** The optimization target must be an observed Rust-vs-TS bridge cost, not an assumption about where Rust is slow. Establish one workload corpus and one result schema before production-path optimization.

**Files:**

- `tools/tui-bench/README.md`
- `tools/tui-bench/types.ts`
- `tools/tui-bench/fixtures.ts`
- `tools/tui-bench/run.ts`
- `tools/tui-bench/compare.ts`
- `crates/iyon-tui/examples/tui_bridge_baseline.rs`
- `packages/iyon-runtime/benchmarks/tui-bridge.ts`

**Work:**

1. Define a versioned JSON result schema containing commit, Rust/Bun versions, terminal dimensions, release/debug mode, warm-up and sample counts, operation phase, workload parameters, p50/p95/p99, allocation/copy counters when exposed, N-API crossing count, and pass/fail status. Keep the schema stable so T12 can consume it.
2. Build fixtures through the same semantic operations exposed to the app: 10/100/1000-node View trees; 10/50/200 stream updates per second; 1k/10k/100k frozen History rows; 1/10/50 Scene transformations; and repeated single-line/multiline/paste TextInput activity. Include nested styles, padding, grids, component handles, open/sealed streams, and non-ASCII text so a fast path cannot accidentally benchmark only trivial ASCII leaves.
3. Time separate phases for View construction, N-API crossing, View materialization, layout, projection, terminal rendering, streaming updates, History promotion, and TextInput activity. Warm each case, run enough repetitions to report distributions, and use a deterministic fake terminal/native sink for repeatability.
4. Make the Rust example call the existing public `iyon-tui` APIs and its existing test/shadow backend; make the Bun runner call the T10 app-facing `iyon:tui` path. Both runners must emit the same fixture identifiers and semantic checksum, not just elapsed time.
5. Add a documented initial acceptance configuration with the 1.25× p95 target and the 10/50/200 update-rate no-drop requirement. The baseline is diagnostic and may fail the final target, but it must be committed before optimization work begins.

**Tests / verification:**

- `cargo fmt --all -- --check`
- `cargo test -p iyon-tui`
- `bun run tools/tui-bench/run.ts --help`
- Run both baseline runners in release mode and assert that every workload/phase has a result, Rust and TS semantic checksums match, and all requested update counts are observed in order.
- Validate the emitted result with `bun run tools/tui-bench/compare.ts --validate <result.json>` and commit the pre-optimization report only after the schema validator passes.

**Must not:**

- Do not change `View`, `History`, stream, Scene, or TextInput public signatures.
- Do not add a benchmark-only native renderer or bypass `iyon-native`.
- Do not claim the exit criterion from one microbenchmark or from mean latency alone.

### Commit 2 — Add bridge phase telemetry and record the baseline

**Why:** Crossing counts and materialization/allocation boundaries are required to distinguish expensive Rust work from avoidable Bun/native marshaling. Instrumentation must exist before the first optimization and must be removable from production builds or disabled by default.

**Files:**

- `crates/iyon-native/src/telemetry.rs`
- `crates/iyon-native/src/lib.rs`
- `crates/iyon-native/src/tui/**`
- `packages/iyon-runtime/src/tui/telemetry.ts`
- `packages/iyon-runtime/src/tui/bridge.ts`
- `tools/tui-bench/fixtures.ts`
- `tools/tui-bench/run.ts`
- `tools/tui-bench/baselines/t11-rust-vs-ts.json`
- `tools/tui-bench/reports/t11-baseline.md`

**Work:**

1. Add test/benchmark-only counters around N-API entry points and phase timers at the existing bridge adapter boundary. Count calls, bytes copied, command/value allocations, native materializations, stream batches, History promotion calls, and TextInput calls; do not log user text, provider data, filesystem paths, or other PII.
2. Expose telemetry through an internal diagnostics object used by the benchmark runner, not through `iyon:tui` public exports. Ensure counters are reset per case and that normal T10 app execution has telemetry disabled or zero-cost guarded instrumentation.
3. Trace the current lazy View path, native-handle path, stream update path, History promotion path, and TextInput path separately. Preserve a legacy/reference implementation switch only inside benchmark/test code so differential tests can compare old and optimized behavior without creating a supported runtime mode.
4. Run the complete workload matrix, record the Rust baseline and current TS overhead, and write the report with the exact accepted threshold. Include a table identifying the top cost center per phase and a decision log for candidates that will not be attempted.

**Tests / verification:**

- `cargo test -p iyon-native` and `cargo test -p iyon-tui`
- `bun test packages/iyon-runtime/src/tui`
- Assert counters reset between cases, disabled telemetry does not alter semantic checksums, and no telemetry field contains fixture text or PII.
- `bun run tools/tui-bench/compare.ts --validate tools/tui-bench/baselines/t11-rust-vs-ts.json`
- Confirm the baseline file exists and contains all 5 workload families × all required sizes/rates × all required phases before proceeding.

**Must not:**

- Do not optimize in this commit.
- Do not expose raw counters as a public extension API or make app behavior depend on telemetry.
- Do not use profiling output to justify changing the 1.25× threshold after the fact.

### Commit 3 — Compile lazy Views into one native materialization call

**Why:** The handoff specifically permits optimizing lazy View materialization into a compact command buffer. This removes per-builder N-API traffic while preserving the public fluent API and the existing Rust layout/materialization engine.

**Files:**

- `packages/iyon-runtime/src/tui/view.ts`
- `packages/iyon-runtime/src/tui/view-command-buffer.ts`
- `packages/iyon-runtime/src/tui/view-command-buffer.test.ts`
- `packages/iyon-runtime/src/tui/bridge.ts`
- `crates/iyon-native/src/tui/view.rs`
- `crates/iyon-native/src/tui/materialize.rs`
- `crates/iyon-native/src/lib.rs`
- `crates/iyon-native/tests/view_materialization.rs`
- `tools/tui-bench/fixtures.ts`

**Work:**

1. Add an internal typed opcode format covering the T5 semantic View tree: text and styled text, horizontal/vertical/grid composition, hanging/container/spacer/clamp nodes, padding and all style/decorative modifiers, and component-handle references. Store strings, styles, and native handles in side tables; use compact integer indices in the command stream. Include a format version and reject unknown versions.
2. Extract/wrap the existing T5 View-to-native conversion so the public `View` builder records semantic operations locally and the final bridge call compiles/materializes once. Preserve modifier order and replacement semantics, especially style state/facts, padding, borders, width/height constraints, and component identity.
3. Add one `materializeView(buffer, strings, styles, handles)` N-API operation. Rust must own/copy all input data before any future suspension, validate every opcode/table index, and lower into existing `iyon_tui::View` constructors/builders. It must not perform layout or rendering itself and must return typed bridge errors rather than panic.
4. Keep a test-only reference materializer that uses the pre-optimization path. Compare semantic output, layout tree, painted rows, component-handle identity, and error cases at multiple widths. The production façade should use the command-buffer path once those comparisons pass.

**Tests / verification:**

- `bun test packages/iyon-runtime/src/tui/view-command-buffer.test.ts`
- Assert golden opcode/table output for `View.text("hello").bold().padding(1)`, nested rows/columns/grids, styles, a component handle, and a mixed Unicode tree.
- `cargo test -p iyon-native view_materialization`
- Assert invalid version/opcode/index/handle errors are typed and deterministic; assert reference and command-buffer paths have identical semantic checksums, layout dimensions, and rendered terminal cells.
- Run the benchmark harness and require a reduction in View-construction/N-API crossings without a regression in layout, projection, or terminal-render timings beyond the recorded threshold.
- `cargo test --workspace` and the T10 Bun build must remain green.

**Must not:**

- Do not change any exported `View` method, `Component`, `IntoView`, `Renderer`, or canonical `iyon:tui` import.
- Do not serialize the View tree as JSON, add a second TS layout engine, or move component state out of native handles.
- Do not delete the reference/test path until differential coverage is established; it must not become a second production implementation.

### Commit 4 — Apply only profile-proven buffer and table reuse

**Why:** Once command-buffer materialization is measured, repeated string/style encoding and short-lived buffer allocation may remain the dominant cost. Reuse is useful only where the baseline proves it; this commit makes that choice explicit and measurable rather than adding speculative caches.

**Files:**

- `packages/iyon-runtime/src/tui/view-command-buffer.ts`
- `packages/iyon-runtime/src/tui/bridge-pool.ts`
- `packages/iyon-runtime/src/tui/intern.ts`
- `packages/iyon-runtime/src/tui/view-command-buffer.test.ts`
- `crates/iyon-native/src/tui/materialize.rs`
- `crates/iyon-native/src/telemetry.rs`
- `tools/tui-bench/compare.ts`
- `tools/tui-bench/reports/t11-buffer-reuse.md`

**Work:**

1. Use Commit 3 telemetry to select the smallest winning mechanism. Prefer per-materialization string/style interning and capacity reuse; use stable subtree caching only if identity, mutable component handles, style resolution, and invalidation can be proven correct. Do not add a global cache by default.
2. Pool only owned scratch buffers/tables whose lifetime is bounded by one synchronous native call. Clear lengths and handle references on return so a pooled buffer cannot retain user text or stale native objects. Keep the public `View` immutable/value-like behavior intact.
3. Add hit/miss/bytes-retained counters and a benchmark report showing the selected mechanism beats the previous command-buffer path on the measured workload. If a candidate does not improve p95 or increases retained memory materially, leave it out and record that decision in the report.

**Tests / verification:**

- `bun test packages/iyon-runtime/src/tui/view-command-buffer.test.ts`
- Assert reused and fresh buffers produce identical command bytes, semantic checksums, layout/paint output, and handle lifetimes; assert the pool is empty after failures.
- Run the full T11 workload matrix twice (cold and warm) and assert no retained string/style/handle data crosses case boundaries.
- Require the final report to show the selected optimization’s p95 and allocation/copy delta; no commit is accepted on allocation reduction alone if rendered output differs.

**Must not:**

- Do not introduce cross-request/global semantic caches, weak identity assumptions, or a public interning API.
- Do not optimize a candidate that profiling did not identify as material.
- Do not retain PII or native handles in a pool after the bridge operation completes.

### Commit 5 — Batch stream updates and History promotion crossings

**Why:** Streaming and History are stateful native-handle paths. At higher update rates, the likely avoidable cost is one JS/native crossing and one promotion attempt per update, not the native scrollback algorithm. Batch ordered updates at the bridge boundary while retaining the existing Rust state machine and acknowledgements.

**Files:**

- `packages/iyon-runtime/src/tui/stream.ts`
- `packages/iyon-runtime/src/tui/history.ts`
- `packages/iyon-runtime/src/tui/bridge-batch.ts`
- `packages/iyon-runtime/src/tui/bridge-batch.test.ts`
- `crates/iyon-native/src/tui/stream.rs`
- `crates/iyon-native/src/tui/history.rs`
- `crates/iyon-native/src/lib.rs`
- `crates/iyon-native/tests/history_stream_batch.rs`
- `crates/iyon-tui/src/history/tests/combined.rs`
- `tools/tui-bench/fixtures.ts`

**Work:**

1. Add an internal batch envelope for the existing handle operations: append/update stream snapshot, advance/seal/compact where already supported, and request bounded History promotion. Preserve sequence numbers, revisions, source coordinates, flow boundaries, terminal insertion acknowledgements, and cancellation ownership.
2. Add one native operation that applies a validated owned batch to the existing `History`/stream handle and invokes the existing transfer functions at the existing points. It must not expose private scrollback state or implement a parallel promotion algorithm.
3. In the TS adapter, coalesce updates only within one event-loop turn or explicit batch call, flush in input order, and flush before a read that requires the latest snapshot. If native reports backpressure or an error, stop at the acknowledged prefix and surface the same error as the unbatched path.
4. Keep an unbatched reference path in tests and compare every intermediate observable snapshot as well as the final terminal tape. Exercise open and sealed streams, compaction, partial sink acknowledgements, viewport following, and history/static/live boundaries.

**Tests / verification:**

- `bun test packages/iyon-runtime/src/tui/bridge-batch.test.ts`
- `cargo test -p iyon-native history_stream_batch`
- Run the existing native-history regressions, including `unstable_stream_prefix_cannot_cross_native_history`, `stream_growth_consumes_bottom_slack_before_native_transfer_on_shadow`, `history_presenter_shadow_tape_remains_contiguous`, and `production_runtime_preserves_native_history_in_its_backend`.
- For 10/50/200 updates per second, assert every update is applied exactly once and in order, no update is dropped, no stream is promoted past its stable frontier, and terminal rows match the reference path.
- Re-run 1k/10k/100k frozen-row and 1/10/50 transformation cases; require fewer crossings or lower p95 without violating the recorded threshold.

**Must not:**

- Do not rewrite `crates/iyon-tui/src/history/native` or change scrollback/frontier semantics.
- Do not batch across unrelated handles, reorder updates, hide backpressure, or turn a failed partial acknowledgement into success.
- Do not let a background task outlive the native handle or make Promise dropping implicitly cancel native work.

### Commit 6 — Optimize repeated TextInput activity only if it is a measured hot path

**Why:** Repeated cursor/edit/paste activity is part of the required workload. If telemetry shows N-API call overhead rather than Rust editing mechanics is material, an ordered native batch can reduce crossings without changing per-event outputs.

**Files:**

- `packages/iyon-runtime/src/tui/text-input.ts`
- `packages/iyon-runtime/src/tui/bridge-batch.ts`
- `packages/iyon-runtime/src/tui/text-input.test.ts`
- `crates/iyon-native/src/tui/text_input.rs`
- `crates/iyon-native/src/lib.rs`
- `crates/iyon-native/tests/text_input_batch.rs`
- `crates/iyon-tui/src/controls/text_input/tests/**`
- `tools/tui-bench/fixtures.ts`

**Work:**

1. Add the batch operation only if Commit 2/5 telemetry identifies repeated crossings as a significant contributor. The batch accepts owned key/edit/paste commands, applies them through the existing `TextInput` command handling in order, and returns the same ordered change/submission/focus outputs plus the final state required by the façade.
2. Preserve canonical newline policy, Unicode cursor byte offsets, kill/yank behavior, multiline wrapping/scroll repair, paste normalization, and submission ordering. Do not expose the internal Rust buffer or replace the existing editor implementation.
3. Flush before any operation that observes or renders current input state. On cancellation/error, return the last acknowledged command index and leave the native handle in the same state as the reference prefix.

**Tests / verification:**

- `bun test packages/iyon-runtime/src/tui/text-input.test.ts`
- `cargo test -p iyon-native text_input_batch`
- Compare batched and per-command paths for repeated insertion, Unicode movement, word deletion, multiline navigation, paste, submit, focus changes, and empty/no-op commands; assert identical output event sequences and cursor/text state after every prefix.
- Run the repeated TextInput benchmark cold and warm. If the measured bridge cost is not material, omit the production batch path and record the no-op decision; the workload and correctness tests still remain reusable.

**Must not:**

- Do not change `TextInput`’s public API or event semantics.
- Do not coalesce commands in a way that removes observable change/submission events or alters error/cancellation boundaries.
- Do not add a generic product event queue; this is a narrow bridge optimization only.

### Commit 7 — Add the final benchmark gate and T12 handoff report

**Why:** T11 exits on measured relative overhead and streaming behavior. A final gate makes the result reviewable and gives T12 deterministic fixtures without moving T12’s packaging or stress responsibilities into this tranche.

**Files:**

- `tools/tui-bench/compare.ts`
- `tools/tui-bench/check.ts`
- `tools/tui-bench/README.md`
- `tools/tui-bench/results/t11-final.json`
- `tools/tui-bench/reports/t11-final.md`
- `tools/tui-bench/types.ts`

**Work:**

1. Add a command that validates the result schema, compares p95 per phase/workload against the committed Rust baseline and 1.25× threshold, verifies all stream update counts/order, and fails with the exact case and metric that missed the gate.
2. Run the complete cold/warm matrix on the optimized bridge in release mode, record build/runtime/environment metadata, and include before/after crossings, allocations/copies, p95 timings, and terminal semantic checksums.
3. Document any retained legacy/reference path, every profile-rejected optimization, and the explicit reason for any metric that cannot meet the threshold. Mark T11 complete only when the TS app’s measured TUI path meets the target and streaming shows no user-visible degradation.
4. Leave the runner and fixtures callable by T12; do not add standalone executable packaging or mandatory release CI here.

**Tests / verification:**

- `cargo fmt --all -- --check`
- `cargo test --workspace`
- `bun test packages/iyon-runtime/src/tui`
- `bun run tools/tui-bench/check.ts tools/tui-bench/results/t11-final.json`
- Assert the final report includes every required workload and phase, passes the 1.25× p95 gate, reports zero dropped/reordered updates at 10/50/200 updates per second, and preserves all Rust/TS semantic checksums.

**Must not:**

- Do not weaken or silently edit the committed threshold to make a failing result pass.
- Do not treat a benchmark pass as permission to change public APIs, native scrollback, or T12 release behavior.
- Do not push the branch.

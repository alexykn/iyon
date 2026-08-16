# Tranche 1 — Bun + N-API viability slice

## Mission

Prove the complete Bun-owned process, Tokio-backed native boundary, virtual-module loading, and standalone-distribution path before binding any real Iyon API, kernel, or TUI surface. The result is a smoke-only Bun CLI that loads an embedded `iyon-native.node`, exercises owned Rust data, async Promise resolution/rejection, explicit cancellation, native finalization, Tokio channels, and one `iyon-tui` link operation, then shuts down cleanly. This tranche establishes the bridge and its verification contract; it does not move product behavior into Rust or begin the API migration.

## Prerequisites

- T0 has replaced `ARCHITECTURE.md` with the Bun + TypeScript + N-API architecture handoff and frozen the dependency graph. The implementation must not repair or rewrite the current document as part of T1.
- The current Cargo workspace, `iyon-api.md`, `iyon-core.md`, `iyon-tui.md`, `iyon.md`, and existing Rust tests are available as migration contracts. Existing crates remain buildable, including the temporary `crates/iyon` compatibility crate and its Rust binary.
- Bun, a supported Node-API version, Rust/Cargo, and the repository’s supported CI targets are available. The exact Bun and Rust toolchain versions used by CI must be recorded by the repository’s existing toolchain policy or, if none exists, pinned in the new CI job rather than inferred from a developer machine.
- T1 must not assume any later tranche has landed. In particular, no API-surface scanner, SDK declarations, kernel extraction, real TypeScript bindings, extension activation registry, migrated provider/tool/agent, or Bun product CLI exists yet.

## Invariants (do not violate)

- Bun owns the process and all application control flow. The deliverable is a TypeScript `iyon-smoke` executable compiled with `bun build --compile`; Rust must not launch Bun, spawn a Bun child process, or become a second application entry point.
- Preserve the Rust dependency boundaries: `iyon-native` may depend on `iyon-api`, `iyon-core`, and `iyon-tui`; it must not make those crates depend on Bun, N-API, or TypeScript. `iyon-native` is a bridge, not a product/application layer.
- T1 native exports are deliberately small probes: `nativeVersion()`, `echoJson()`, `asyncSleep()`, `CancellationProbe`, `NativeCounter`, `EventQueueProbe`, and one trivial `iyon-tui` operation. They must not expose or refactor real protocol, kernel, provider, tool, agent, history, or TUI APIs.
- Every value crossing an async boundary is owned. No N-API borrowed value, JS object reference, or borrowed Rust data may be retained while a Tokio future is suspended. Clone or convert input into owned Rust data before spawning/awaiting and create fresh owned N-API values at the completion boundary.
- Exported native functions return `napi::Result<T>`/typed Promise rejection paths. No `unwrap`, `expect`, or casual panic may cross into Bun; background tasks must convert expected failure, closure, cancellation, and join outcomes into stable native errors.
- Every long-lived operation has an owner and explicit cancellation/close path. Dropping a JS Promise is not treated as cancellation. `AbortSignal` is bridged by an explicit `cancel()` call, and native `Drop` implementations cancel/close owned work before releasing state.
- Tokio tasks may not outlive the native handle/runtime state they reference. A task may retain only an `Arc` of state whose lifetime is independent of the JS wrapper, and the wrapper’s drop/close path must make receiver futures terminate.
- The virtual loader is installed before any application import of `iyon:api`, `iyon:core`, or `iyon:tui`. Resolution/loading uses Bun `onResolve`/`onLoad`; it is not a filesystem alias, child-process protocol, or separate built-in extension loader.
- Native TUI state is not serialized into a duplicate TypeScript renderer. The one TUI probe only proves that `iyon-native` links and can call `iyon-tui`.
- Keep `crates/iyon` and current Rust behavior intact. T1 does not delete `main.rs`, replace the Rust executable, or change existing public Rust APIs.

## Out of scope

- T2’s `tools/api-surface/` scanner, disposition inventory, `@iyon/sdk`, or generated declarations.
- T3’s `iyon-core` kernel extraction, removal of `register_builtin_defaults`, or changes to provider/tool ownership.
- T4/T5’s real `iyon:api`, `iyon:core`, and `iyon:tui` bindings, API parity, lazy semantic values, native handles, and TUI runtime integration. T1’s TUI operation is a placeholder only.
- T6’s extension/package model, transactional activation, contribution registry, duplicate-ID/replacement policy, or `iyon:plugins` implementation. T1 must not add a plugin loader or a built-in-vs-third-party branch.
- T7–T9’s TypeScript providers, tools, default agent, and product behavior.
- T10’s real CLI, auth/provider startup semantics, UX parity, canonical `iyon` executable, or removal of the Rust product binary. The T1 command is named `iyon-smoke` and is smoke-only.
- T11’s TUI bridge optimization and benchmarks, except for the correctness probes needed to establish the initial boundary.
- T12’s release packaging, signing, installer, ecosystem policy, and broad hardening beyond the target matrix and boundary tests below.
- WASM, WIT, Rhai, a custom JS engine, embedded Bun in Rust, a Rust-owned process, a Rust HTTP/provider layer, or any fallback child-process architecture.

## Adjacent tranche contracts

- **Consumes from T0:** the Bun-owned runtime decision, the Rust graph, canonical virtual import names, native-boundary rules, standalone `.node` embedding requirement, and the frozen “no real bindings in T1” scope. It also consumes the existing Rust crates and their current tests as the baseline.
- **Exports for later tranches:** a root Bun workspace and lockfile; a buildable `crates/iyon-native` N-API addon with explicit ownership/cancellation/error semantics; a stable native smoke contract; an idempotent virtual-module installation seam; smoke-only `iyon:api`, `iyon:core`, and `iyon:tui` import paths; a Bun CLI build that embeds the addon; and a target-matrix verification harness.
- **Must not steal from neighbors:** T2 owns exhaustive API accounting and SDK types; T3 owns core extraction; T4/T5 own real module surfaces; T6 owns package/extension activation and `iyon:plugins`; T7–T10 own migrated product behavior. If those later tranches need a placeholder, T1 provides only the named smoke symbols and loader seam, not their eventual implementation.
- **Later-tranche stubs:** `iyon:api`, `iyon:core`, and `iyon:tui` may export names such as `apiSmoke`, `coreSmoke`, and `tuiSmoke` plus the native probes needed by the smoke command. Their declarations must mark them as T1 smoke-only and make no promise of protocol/kernel/TUI parity. Do not add `iyon:plugins` in T1; T6 extends this installer to register that module.

## Commits

### Commit 1 — Establish the Bun workspace baseline

**Why:** Create the minimum TypeScript/Bun project shape without coupling it to unfinished native or product work. This gives later commits a lockfile, strict compiler settings, and workspace package identity while leaving the existing Rust application unchanged.

**Files:**

- `package.json`
- `bun.lock`
- `tsconfig.json`

**Work:**

- Mark the root package private and define Bun workspaces for `packages/*`; reserve plugin directories for later tranches rather than adding them now.
- Add only the toolchain metadata needed for T1: Bun scripts for install, typecheck, native staging/build, smoke tests, and standalone build; workspace references use `@iyon/runtime` and `@iyon/cli` names once those packages exist.
- Configure strict TypeScript checking with Bun-compatible bundler resolution, ES modules, and declarations for `*.node` imports and the three `iyon:*` module names. Keep emit disabled for typechecking; Bun remains the runtime/bundler.
- Generate and commit `bun.lock` with `bun install --lockfile-only`/the repository-approved equivalent. Do not add SDK, plugin, provider, or application dependencies.

**Tests / verification:**

- `bun install --frozen-lockfile` succeeds from a clean checkout.
- `bunx tsc --noEmit` is either green with no TS sources yet or reports no configuration errors; the final commit must not rely on `skipLibCheck` or an implicit `any` escape hatch.
- `cargo test --workspace` remains green and `cargo check --workspace` confirms that the Rust baseline is unchanged.

**Must not:** Add `crates/iyon-native`, package source, a native build artifact, virtual loader behavior, a new dependency unrelated to T1, or any edit to `ARCHITECTURE.md`.

### Commit 2 — Add the synchronous N-API bridge and typed boundary

**Why:** Make `iyon-native` a real workspace member and prove that Bun can load a native Rust addon linked against the three existing Rust libraries before async lifetime complexity is introduced.

**Files:**

- `Cargo.toml`
- `Cargo.lock`
- `crates/iyon-native/Cargo.toml`
- `crates/iyon-native/src/lib.rs`
- `crates/iyon-native/src/error.rs`
- `crates/iyon-native/src/sync.rs`
- `crates/iyon-native/src/tui.rs`
- `crates/iyon-native/tests/sync.rs` (or the crate’s equivalent Rust integration-test location)

**Work:**

- Add `crates/iyon-native` to the Cargo workspace as a `cdylib` N-API addon. Add the napi-rs `napi`/`napi-derive` pair with the required Tokio runtime and serde-JSON support, plus `napi-build` only if required by the selected napi-rs major. Resolve and commit versions in `Cargo.lock`; do not change existing crate dependency edges.
- Register the addon module with napi-rs and keep all exported functions in small bridge modules. Put conversion/validation failures behind a local `NativeError`/`napi::Error` mapping with stable categories such as invalid input and internal failure; do not expose `anyhow::Error` or provider errors directly.
- Implement `nativeVersion() -> string` as a stable T1 marker and `echoJson(value) -> value` by converting the incoming JS value to owned `serde_json::Value`, then returning an owned value. Cover objects, arrays, strings, numbers, booleans, and null; reject unsupported/cyclic input through a typed N-API error.
- Implement one `tuiSmoke()` operation that constructs a minimal `iyon_tui::View` value through its public API and returns a stable marker. This is only a link/surface probe; it must not serialize or reimplement `View`.
- Add code comments at the exact N-API/async boundary documenting owned conversion, typed errors, task ownership, and the fact that this crate must not accumulate product behavior.

**Tests / verification:**

- `cargo fmt --all -- --check`, `cargo check -p iyon-native`, and `cargo test -p iyon-native` pass on the host target.
- Native unit/integration tests cover JSON round trips, unsupported input rejection, large owned `String` conversion, and the TUI smoke marker without changing `iyon-api`, `iyon-core`, or `iyon-tui`.
- The napi-rs build/stage command produces a platform-native `iyon-native.node` (or the platform-specific staged filename used by the build script) and a minimal Bun probe can load it and observe `nativeVersion()`/`tuiSmoke()`.

**Must not:** Refactor `iyon-core` or `iyon-tui`, expose real Rust API inventories, add a Rust `main()`, add providers/tools/agents, or turn `iyon-native` into an application/service layer.

### Commit 3 — Implement owned async, cancellation, handle, and channel probes

**Why:** Exercise the exact N-API operations Iyon will depend on instead of accepting a trivial addon load as evidence that Bun’s incomplete Node-API support is sufficient.

**Files:**

- `crates/iyon-native/src/lib.rs`
- `crates/iyon-native/src/error.rs`
- `crates/iyon-native/src/async_ops.rs`
- `crates/iyon-native/src/handles.rs`
- `crates/iyon-native/src/queue.rs`
- `crates/iyon-native/tests/async_ops.rs`
- `crates/iyon-native/tests/handles.rs`
- `crates/iyon-native/tests/queue.rs`

**Work:**

- Implement `asyncSleep(ms)` as a napi-rs async export using the addon’s Tokio integration. Convert the input before suspension, return a successful Promise for bounded delays, and reject immediately with a typed invalid-input error for an over-limit value so Promise failure is testable without a long test sleep.
- Implement `CancellationProbe` as an N-API class with an owned cancellation token, `run(ms)`/wait behavior, `cancel()`, and a close/drop path. Cancellation must resolve to a stable cancellation error/result, and dropping the JS wrapper must signal cancellation without relying on Promise garbage collection. Keep no JS object or borrowed N-API value inside the future.
- Implement `NativeCounter` as an N-API class backed by owned native state with `increment()` and `value()`. Track live/finalized instances in test-only-safe observable counters or a diagnostic function so the finalizer path can be asserted; the production class must not leak a Tokio task or runtime reference.
- Implement `EventQueueProbe` with a Tokio `mpsc` sender owned by the JS handle and a receiver held behind owned shared state. Expose `send(event)`, `nextEvent() -> Promise<event | null>`, and `close()`. `nextEvent` must resolve on a queued owned JSON event or channel close, and close/drop must make an idle receiver terminate.
- Add a small native shutdown helper or equivalent deterministic close protocol used by the queue and cancellation probes. Ensure all spawned work is joined/observably terminated before native state is released; do not leave detached tasks that retain the addon or runtime.
- Add concurrency coverage for 100 simultaneous Rust futures and channel ordering. Use `tokio::time::pause`/short bounded delays where useful, but retain at least one real Promise completion path in the Bun integration tests.

**Tests / verification:**

- Rust tests cover `async_sleep_resolves`, `async_sleep_rejects_invalid_delay`, `cancellation_probe_stops_owned_operation`, `counter_increments`, `event_queue_preserves_fifo_order`, `event_queue_close_resolves_waiter`, and `one_hundred_concurrent_futures_complete`.
- The native boundary tests assert that cancellation and closed queues are typed failures/terminal values, not panics, and that no task accesses a dropped handle’s state.
- Build and load the addon with Bun on each development target; a load-only test is insufficient—the test must invoke the async, class, JSON, Buffer/string, and channel operations.

**Must not:** Add a callback-driven Rust-to-JS event architecture, use Promise dropping as cancellation, hold N-API borrows over `.await`, expose real core/TUI handles, or change existing runtime cancellation semantics in `iyon-core`.

### Commit 4 — Add `iyon-runtime` and the pre-import virtual module loader

**Why:** Prove that canonical `iyon:*` imports are resolved by a Bun runtime plugin before application imports and that the TypeScript façade can bridge `AbortSignal` to an explicit native cancellation token.

**Files:**

- `packages/iyon-runtime/package.json`
- `packages/iyon-runtime/src/index.ts`
- `packages/iyon-runtime/src/native.ts`
- `packages/iyon-runtime/src/virtual-modules.ts`
- `packages/iyon-runtime/src/smoke.ts`
- `packages/iyon-runtime/src/virtual-modules.d.ts`
- `packages/iyon-runtime/test/virtual-modules.test.ts`
- `packages/iyon-runtime/test/native-boundary.test.ts`
- `package.json` (workspace scripts/dependencies only)
- `bun.lock`
- `tsconfig.json` (module declarations/options only)

**Work:**

- Create the `@iyon/runtime` workspace package with strict source exports and a single native-addon import seam. The native staging/build script must place the target-specific addon at an explicit generated path such as `packages/iyon-runtime/native/iyon-native.node`; do not fall back to a child process or dynamically locate a different executable.
- Implement `installIyonVirtualModules()` as idempotent, install-once state. Register Bun `onResolve` filters for exactly `iyon:api`, `iyon:core`, and `iyon:tui`, route them to a private virtual namespace, and use `onLoad` to return TypeScript source that re-exports only T1 smoke symbols. Verify that registration happens before the first dynamic import.
- Keep the smoke exports intentionally small: `iyon:api` exposes the API marker plus JSON/string/Buffer probes, `iyon:core` exposes async/cancellation/queue probes, and `iyon:tui` exposes the TUI marker. Do not claim real protocol/kernel/TUI declarations and do not add `iyon:plugins`.
- Add a typed `runWithAbortSignal(signal, operation)` helper that registers an abort listener, calls the native `cancel()` method explicitly, removes the listener in a `finally`, and preserves native Promise success/failure. Document that an unreferenced Promise alone does not cancel Rust work.
- Define TypeScript types for the smoke results, native error categories, event payloads, and native class method contracts. Keep Buffer crossing as a direct Bun `Buffer`/N-API value, with a separate large-string test; do not JSON-encode binary data.

**Tests / verification:**

- `bun run typecheck` passes with strict settings and no ambient `any` for the virtual modules.
- `virtual_modules_load_before_application_imports` calls the installer, dynamically imports all three canonical names, and asserts each smoke marker.
- `json_round_trip_preserves_nested_values`, `large_string_transfer_is_lossless`, and `buffer_transfer_preserves_bytes` exercise real addon conversions.
- `promise_success_and_failure_are_distinct`, `abort_signal_calls_explicit_native_cancel`, and `idle_event_receiver_closes_cleanly` assert Promise/rejection/cancellation behavior and cleanup.

**Must not:** Add a package registry, extension activation, `iyon:plugins`, real SDK/API declarations, a separate built-in loader, a JSON TUI renderer, or imports that happen before plugin installation.

### Commit 5 — Build the smoke-only Bun CLI and embedded standalone executable

**Why:** Close the end-to-end distribution loop: Bun must own a compiled executable whose statically included native addon reaches Tokio/Rust and returns to TypeScript without a fallback process.

**Files:**

- `packages/iyon-cli/package.json`
- `packages/iyon-cli/src/index.ts`
- `packages/iyon-cli/src/smoke-command.ts`
- `packages/iyon-runtime/scripts/stage-native.ts`
- `packages/iyon-runtime/native/.gitignore` (generated addon/output policy)
- `package.json` (native stage, smoke, and standalone scripts only)
- `bun.lock`

**Work:**

- Create the `@iyon/cli` workspace package with the `iyon-smoke` bin entry. Its entry point must install the runtime plugin before importing any `iyon:*` module, run the smoke command, print machine-readable results, and return a nonzero exit status for any failed assertion.
- Add a platform-aware native staging script that invokes the pinned napi-rs build flow for `crates/iyon-native`, verifies the produced artifact is a `.node` addon for the current target, and stages it at the runtime package’s explicit import path. Never select a Rust executable or spawn a Rust child process.
- Add the standalone script using `bun build --compile` with `packages/iyon-cli/src/index.ts` as the entry point and `iyon-smoke` as the output. Keep the native addon as a static import reachable from the bundle so Bun embeds `iyon-native.node`; do not copy it beside the executable as a runtime-only requirement.
- Make the smoke command run, in one clean process, native version/TUI probes, nested JSON, large string, Buffer, Promise success/failure, 100 concurrent sleeps, explicit `CancellationProbe` cancellation through `AbortSignal`, `NativeCounter` finalization polling under the supported Bun GC test mode, FIFO `EventQueueProbe`, close/shutdown, and a final no-hanging-tasks check.
- Keep CLI output and assertions smoke-specific and deterministic. Do not add auth, provider selection, terminal startup, interactive commands, product configuration, or Rust compatibility behavior.

**Tests / verification:**

- `bun run native:stage` produces the expected target addon and fails clearly if the target is unsupported or the artifact is not loadable.
- `bun run smoke` runs the source Bun CLI and asserts all critical probes, including success and failure Promise paths, cancellation, finalization, JSON, large `String`, `Buffer`, 100 concurrent futures, channel receiver, and clean shutdown.
- `bun run build:standalone` runs `bun build --compile`; executing the resulting `iyon-smoke` binary reproduces the same assertions from a directory without the staged addon beside it.
- The standalone test inspects process exit/status and output, and verifies that no Rust executable/child-process path is involved. A successful addon load alone is not an acceptance test.

**Must not:** Rename the product CLI to `iyon`, migrate the Rust `main.rs`, add application/provider/tool/agent behavior, use a fallback child process, or make the standalone executable depend on an unembedded `.node` file.

### Commit 6 — Add target-matrix conformance CI and T1 handoff checks

**Why:** Make the viability claim reproducible on every supported target and prevent a future change from reducing validation to “the trivial addon loads.”

**Files:**

- `.github/workflows/t1-bun.yml`
- `packages/iyon-runtime/test/native-probes.test.ts`
- `packages/iyon-cli/test/standalone.test.ts`
- `packages/iyon-runtime/scripts/verify-native.ts` (only if a separate cross-platform verifier is needed)
- `package.json`/`bun.lock` only for test script wiring

**Work:**

- Add a CI matrix for the project’s supported Bun/native targets, with at least Linux x64 and macOS arm64 if those are the currently supported release targets; include other targets only when already supported by project policy. Do not silently broaden product support by adding an unverified platform.
- Each matrix job checks out the repository, installs the pinned Bun/Rust toolchains, runs `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo test --workspace`, Bun lockfile installation, native staging, strict TypeScript typecheck, source smoke tests, standalone compilation, and execution of the compiled `iyon-smoke`.
- Split the Bun tests into named assertions covering the complete T1 list: `async_rust_resolves_to_promise`, `promise_failure_is_typed`, `explicit_cancellation_stops_work`, `native_counter_finalizer_runs`, `json_conversion_round_trips`, `large_string_transfer`, `buffer_transfer`, `one_hundred_concurrent_futures`, `tokio_channel_receiver`, `clean_shutdown`, and `addon_is_embedded_in_standalone_executable`.
- Use bounded polling/timeouts for finalizer and shutdown assertions, report target/Bun/Rust versions on failure, and fail if any test requires a sibling addon or invokes a Rust process. Keep flaky GC/finalizer handling explicit and target-aware; do not delete the test merely because finalization is nondeterministic.
- Record the T1 exit checklist in CI output: Bun owns the process; the compiled executable reaches embedded native addon → Tokio/native Rust operation → TypeScript continuation; all smoke module imports work; no real API/kernel/TUI binding is claimed.

**Tests / verification:**

- CI is green across every declared target, with the standalone binary executed rather than only built.
- A clean checkout can reproduce the local sequence: `bun install --frozen-lockfile`, native stage, `bun run typecheck`, `bun run test`, `bun run build:standalone`, and execute the generated `iyon-smoke`.
- `git diff --check` and `git status --short` show no generated addon, `dist/` binary, or unrelated source changes committed.

**Must not:** Add release packaging/signing, publish artifacts, real extension tests, API-surface parity checks owned by T2, or a broad product support claim not backed by the existing target policy.

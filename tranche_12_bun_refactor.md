# Tranche 12 — Hardening, packaging, ecosystem readiness

## Mission

Make the Bun-owned Iyon runtime safe under extension, streaming, cancellation, and shutdown pressure; ship the Bun standalone executable with the correct native addon on every supported target; publish a static, metadata-only `iyon.catalog` pipeline; and make the extension platform usable and verifiable by third-party authors. This tranche closes the T0–T11 migration with failure isolation, deterministic stress coverage, release automation, documentation, and architectural acceptance tests. It does not reopen the Bun + TypeScript + N-API architecture or perform the later breaking Rust compatibility release.

## Prerequisites

T12 starts only after the exit criteria for T0–T11 are green on the same tree:

- T0 replaced the obsolete architecture document and froze the Bun-owned process, Rust library, N-API bridge, extension, registry, Scene, and compatibility boundaries.
- T1–T5 provide `iyon-native`, the canonical `iyon:api`, `iyon:core`, and `iyon:tui` paths, the SDK/API-surface tracker, and the working native TUI/runtime handles.
- T6 provides one transactional package/extension loader for bundled and third-party packages, duplicate-ID rejection, explicit replacement, contribution registries, activation rollback, and unload handles.
- T7–T9 provide TypeScript providers, tools, and agents using the same registration and kernel APIs as third-party code.
- T10 makes the Bun TypeScript CLI/default app the canonical `iyon` product while retaining `crates/iyon` as a deprecated Rust compatibility library; its provider/auth/CLI behavior and the existing Rust public-app tests remain the behavioral oracle.
- T11 has completed functional TUI parity and any bridge batching/optimization only after parity was demonstrated.

T12 must not assume work from a later breaking-release tranche. In particular, `crates/iyon` still exists, deprecated Rust provider types may still exist for compatibility, and the future iyon.dev site may consume the catalog contract but is not yet a deployed website.

## Invariants (do not violate)

- Bun owns process control, product behavior, extension activation, and continuations. Do not restore a Rust `main()` that launches Bun, embed Bun inside Rust, or add a Rust-owned provider HTTP/application layer.
- The native bridge is only a bridge to `iyon-api`, `iyon-core`, and `iyon-tui`. Product policy and extension behavior stay in TypeScript. The Rust dependency graph remains `iyon-api → iyon-core`, independent `iyon-tui`, and `iyon-native → iyon-api + iyon-core + iyon-tui`.
- Canonical extension imports remain `iyon:api`, `iyon:core`, `iyon:tui`, and `iyon:plugins`; `@iyon/sdk` remains the development/type package. Do not introduce WASM, Rhai, WIT, a custom JavaScript engine, or a second plugin runtime.
- Packages, extensions, and contributions remain distinct. Tool/provider/agent/app are contribution metadata, not restrictive plugin base classes. An arbitrary TypeScript extension may use normal Bun/npm, filesystem, process, network, command, session, and Scene APIs from one entrypoint.
- The same loader and activation transaction handle bundled and third-party extensions. There is no `is_builtin` fast path, hidden default registration, provider/tool allowlist, or privileged built-in agent API.
- Activation is atomic: an activation, registration callback, or unload failure cannot leave half of an extension’s registries, hooks, Scene layers, event subscriptions, or task owners installed. Duplicate IDs fail by default; `replace: true` is explicit and layered, and unloading a replacement restores the previous live layer.
- Every extension-facing failure carries package and extension provenance, preserves a typed cause, and is surfaced without taking down unrelated registries, the terminal state, or the host process. Do not swallow errors or use a global catch that hides the failing extension.
- Every long-lived native operation has an explicit owner and cancellation path. `AbortSignal`/explicit cancellation must reach Rust cancellation tokens; dropping a Promise alone is not cancellation. Native tasks must not outlive the handles/runtime they use, and Rust panics must become typed errors rather than unwind through Bun.
- Native scrollback, streaming smoothness, history promotion, composer placement, approval transitions, and shutdown behavior remain unchanged. The existing Rust public-app tests and TUI harness are behavioral evidence, not permission to duplicate terminal/layout logic in TypeScript.
- Replacing a builtin tool, adding a provider, adding an agent, selecting a third-party app, and replacing a selected app’s Scene must work through registration metadata and public APIs without default-app special cases.
- `iyon-api`, `iyon-core`, and `iyon-tui` remain usable by a pure Rust consumer without Bun. Every reachable public API has a machine-tracked TypeScript path; the API-surface report must remain `missing=0` and `stale=0` with no permanent Ignore/Unsupported escape hatch.
- Catalog generation reads package manifests and static metadata only. It must never import, bundle, evaluate, or execute third-party package JavaScript to build `iyon.catalog`.
- Distribution uses Bun’s standalone executable mechanism with the native `.node` addon embedded. A shipped `iyon` must contain Bun and load the addon without a separate JavaScript runtime installation.
- T12 does not delete `crates/iyon`, remove deprecated Rust concrete providers, deploy the full iyon.dev website, or perform the later intentional breaking API wipe.

## Out of scope

- Reopening architecture, changing runtime ownership, or adding WASM, Rhai, WIT, a Rust-owned process, a second extension loader, or a custom npm registry.
- New product features, new default tools/providers/agents/apps, or changing provider/auth semantics beyond the hardening and metadata needed to test the already-migrated product.
- Removing `crates/iyon`, deleting deprecated Rust provider types, or changing public Rust compatibility APIs for the later breaking release.
- Building a sandbox or pretending that normal Bun extensions are capability-safe. The security work is the documented trust/approval model and provenance-aware failure behavior; mandatory filesystem/network APIs are not added here.
- Reimplementing the TUI in TypeScript, changing native scrollback/layout algorithms, or optimizing the bridge without a T11 benchmark and parity baseline.
- Building a complete deployed iyon.dev website. T12 supplies a versioned catalog schema and deterministic index pipeline for a future site.
- Adding dependencies, release services, marketplace connectors, or external packages without the repository’s required approval and dependency review.

## Adjacent tranche contracts

T12 consumes the native binding, API-surface, provider-agnostic kernel, TUI handles, transactional loader, migrated bundled contributions, canonical Bun CLI, and post-parity bridge from T1–T11. It exports the following stable contracts for later maintenance and the eventual breaking release:

- a provenance-bearing, failure-isolated extension lifecycle with deterministic cleanup, replacement, unload, cancellation, and shutdown semantics;
- a stress/regression suite and architectural acceptance matrix that later changes must keep green;
- a supported-target manifest, reproducible Bun standalone build, embedded-addon smoke check, artifact metadata, and CI release workflow;
- the versioned, static `iyon.catalog` manifest contract and metadata-only generation rule;
- author-facing documentation and canonical bundled-extension examples for packages, tools, providers, agents, apps, Scenes, low-level core/TUI access, installation, and security.

T12 must not steal from neighboring work: T11 owns bridge performance decisions, the later breaking-release tranche owns Rust API removal, and the future iyon.dev work owns site deployment and presentation. If a later tranche needs a temporary compatibility stub, it must consume the public contracts above rather than add a new T12-only privileged path.

## Commits

### Commit 1 — Add provenance-aware extension fault isolation

**Why:** The loader and contribution registries are now shared by the bundled product and arbitrary npm extensions. T12 needs a single failure boundary that can identify the package/extension at fault and roll back only its owned state, so one bad extension cannot corrupt other registrations or terminal state.

**Files:** `packages/iyon-runtime/src/provenance.ts`, `packages/iyon-runtime/src/errors.ts`, `packages/iyon-runtime/src/activation.ts`, `packages/iyon-runtime/src/registry.ts`, `packages/iyon-runtime/src/lifecycle.ts`, `packages/iyon-runtime/src/index.ts`, `packages/iyon-runtime/test/isolation.test.ts`, `packages/iyon-plugins/src/loader.ts`, `packages/iyon-plugins/test/loader-isolation.test.ts`

**Work:**

1. Define a serializable `PackageProvenance`/`ExtensionProvenance` value containing package name/version, extension ID, contribution kind when known, and a stable package location or manifest reference. Define `ExtensionError` with phase (`discover`, `activate`, `register`, `event`, `dispose`, or `shutdown`), provenance, typed cause, and safe diagnostic fields; preserve causes and redact tokens, credentials, and arbitrary argument payloads from user-facing output.
2. Extract the existing T6 activation bookkeeping into an explicit `ActivationTransaction`/`ContributionOwner` boundary. Every registry entry, replacement layer, hook, Scene layer, app/provider/tool/agent metadata record, event subscription, and task owner gets an owner token. Commit publishes all staged state together; rollback removes only staged ownership and invokes already-created disposers in reverse order.
3. Make duplicate registration fail before mutation unless the request carries `replace: true`. Store replacement stacks rather than overwriting a map entry, and make an unload handle restore the previous layer atomically. A failed replacement must leave the original contribution live.
4. Wrap extension entrypoint, registration callback, event handler, and disposer exceptions at the host boundary. Convert them to `ExtensionError`, record them through the runtime diagnostic sink, and continue only where the contract permits continuation. Never let an extension exception escape into registry mutation or the TUI update loop.
5. Keep the existing T6 loader path for bundled and third-party packages; route both through the same provenance and transaction code. Do not introduce a builtin branch or make error isolation depend on package trust.

**Tests / verification:** `bun test packages/iyon-runtime/test/isolation.test.ts packages/iyon-plugins/test/loader-isolation.test.ts`; assert `activation_failure_rolls_back_all_contributions`, `extension_exception_does_not_corrupt_registry_or_scene`, `duplicate_ids_fail_without_replace`, `failed_replace_leaves_previous_owner_live`, `unload_restores_previous_layer`, and `diagnostics_include_package_and_extension_provenance_without_secrets`. Run the TypeScript typecheck for the touched workspace packages and confirm a clean activation still reaches the existing default app/TUI path.

**Must not:** Change the contribution model, add privileged builtins, swallow exceptions, modify native task ownership, or alter the public Rust compatibility crates.

### Commit 2 — Add deterministic runtime stress and race coverage

**Why:** Registration correctness in a small unit test is not enough for the specified extension pressure. This commit turns the failure-isolation contract into repeatable tests for concurrent registrations, replacement churn, activation failures, streaming failures, approval failures, cancellation storms, and shutdown with outstanding work.

**Files:** `packages/iyon-runtime/test/stress/runtime-stress.test.ts`, `packages/iyon-runtime/test/stress/fixtures.ts`, `packages/iyon-runtime/test/stress/fake-extension.ts`, `packages/iyon-runtime/src/test-support.ts`, `packages/iyon-cli/test/stress/runtime-cli-stress.test.ts`, `tests/fixtures/extensions/*`, `tests/fixtures/providers/*`, `tests/fixtures/tools/*`

**Work:**

1. Add deterministic barriers, fake clocks, and failure-injection fixtures rather than timing-based sleeps. The fixture extension should register a contribution, a session-event handler, a Scene layer, and an unload disposer, and expose counters for activation, disposal, event delivery, and leaked tasks.
2. Exercise 100 extensions activated concurrently, many simultaneous registrations, repeated override/unload cycles, duplicate IDs, and an injected activation failure at each phase. After every run assert registry ownership, replacement-stack order, Scene resolution, event-subscription count, disposer count, and absence of unhandled promise rejections.
3. Drive at least 50 concurrent native-backed async operations through the public runtime façade, then issue cancellation storms while operations are queued, streaming, awaiting approval, and completing. Assert each operation resolves exactly once as completed, cancelled, or typed failure and that no task/handle remains owned by a disposed extension.
4. Use scripted provider/tool fixtures to force provider failure during streaming and tool failure during approval. Assert partial assistant/tool state is finalized consistently, the failing extension/provider/tool provenance is preserved, unrelated registrations survive, and the TUI receives no orphaned approval or stream item.
5. Shut down while registrations, streams, approvals, and native operations remain outstanding. Assert shutdown waits for owned work or cancels it according to the lifecycle contract, then leaves zero active tasks, receivers, subscriptions, and native handles. Make the test repeatable under a bounded loop count so CI is useful and not flaky.

**Tests / verification:** `bun test packages/iyon-runtime/test/stress/runtime-stress.test.ts packages/iyon-cli/test/stress/runtime-cli-stress.test.ts`; run the stress suite repeatedly with the repository’s Bun test runner and an explicit deterministic seed. Required test names include `activates_100_extensions_without_registry_corruption`, `concurrent_registration_and_replace_is_atomic`, `activation_failures_leave_no_owned_state`, `cancellation_storm_resolves_each_operation_once`, `provider_failure_during_stream_isolated`, `tool_failure_during_approval_isolated`, and `shutdown_drains_outstanding_work`. Capture and assert zero leaked owners/tasks/subscriptions in the fixture summary.

**Must not:** Relax cancellation semantics, add sleeps to hide races, change TUI layout behavior, or make the tests pass by disabling extension exceptions.

### Commit 3 — Harden N-API operation ownership and shutdown

**Why:** The runtime stress tests need a native boundary that cannot outlive its handles or silently ignore cancellation. This commit hardens the T1/T4/T5 bridge and the TypeScript façade around the existing kernel/TUI operations, including the equivalent of the current `CoreBridge::shutdown` behavior.

**Files:** `crates/iyon-native/src/error.rs`, `crates/iyon-native/src/operation.rs`, `crates/iyon-native/src/runtime.rs`, `crates/iyon-native/src/lib.rs`, `crates/iyon-native/tests/lifecycle.rs`, `packages/iyon-runtime/src/native/operation.ts`, `packages/iyon-runtime/src/native/shutdown.ts`, `packages/iyon-runtime/test/native-lifecycle.test.ts`

**Work:**

1. Introduce an explicit Rust `OperationOwner`/task-group abstraction for long-lived model streams, core commands, tool executions, TUI event receivers, and shutdown. Keep cancellation tokens and join handles together; cancel first, stop accepting new work, drain/close receivers, and join every owned task before releasing runtime/native state.
2. Extract the cancellation and join logic from the prior native operation wrappers and event bridge into reusable helpers. Preserve the current event mapping, batching, approval, stream, and native-history contracts; do not move application decisions into `iyon-native`.
3. Add typed N-API errors with operation name, lifecycle phase, cancellation status, and safe cause text. Convert Rust panic/JoinError/channel-close cases into rejected promises with stable error codes; never unwind through Bun and never leave a Tokio task holding borrowed N-API values across an await.
4. Bridge `AbortSignal` and explicit runtime cancellation to the Rust token exactly once, make cancellation idempotent, and ensure a rejected/cancelled Promise cannot cause a later callback into a disposed JS object. Keep data/value crossings for one-shot events and native handles for stateful History, TextInput, stream, and app state.
5. Make shutdown idempotent and observable for tests: repeated shutdown returns the first result, outstanding work is accounted for, and the native addon remains loadable after unrelated extension failure until the host intentionally exits.

**Tests / verification:** `cargo test -p iyon-native --test lifecycle` and the touched Rust lint/type checks; `bun test packages/iyon-runtime/test/native-lifecycle.test.ts`. Assert `abort_signal_cancels_native_operation`, `native_panic_becomes_typed_rejection`, `shutdown_joins_every_owned_task`, `shutdown_is_idempotent`, `receiver_close_does_not_leak`, and `cancelled_operation_never_calls_disposed_js_handle`. Run the existing Rust workspace tests that cover core cancellation, streaming, approval, and TUI shutdown to prove the bridge contract remains intact.

**Must not:** Add a Rust `main`, a Rust HTTP/provider layer, arbitrary JS callbacks from background tasks, JSON serialization of native TUI state, or a second runtime/loader.

### Commit 4 — Build and locally verify Bun standalone executables

**Why:** The canonical product is now the TypeScript CLI, so release correctness must be defined by a standalone Bun executable that embeds and loads `iyon-native`, not by a developer machine’s separate Bun installation or a Rust compatibility binary.

**Files:** `packages/iyon-cli/src/distribution.ts`, `packages/iyon-cli/src/cli.ts`, `packages/iyon-cli/scripts/build-standalone.ts`, `packages/iyon-cli/scripts/verify-standalone.ts`, `packages/iyon-cli/test/distribution.test.ts`, `packages/iyon-cli/package.json`, `package.json`, `scripts/release/targets.ts`, `scripts/release/manifest.ts`, `dist/.gitkeep`

**Work:**

1. Declare one source-of-truth supported-target matrix in `scripts/release/targets.ts`, including OS/architecture, Bun target, Rust target, native addon filename/ABI, executable name, and whether the target can be executed on the build runner. Fail closed when a target has no matching native artifact.
2. Add a release builder that builds `crates/iyon-native` in release mode for the selected target, stages the `.node` addon and its metadata, and invokes Bun’s standalone executable mechanism for the T10 TypeScript CLI. Keep virtual `iyon:*` module resolution and workspace package entrypoints in the compiled graph; do not require a runtime JS install at execution time.
3. Emit a machine-readable release manifest containing version, target, Bun version, Rust/native artifact identity, source revision, and checksums. Do not include credentials, local absolute paths, or a second copy of package JavaScript outside the executable’s intended embedded payload.
4. Add a local verifier that runs `iyon --version`/`--help`, starts a headless native smoke command, imports a native operation, performs one provider-agnostic core operation and one TUI/native-handle operation, and proves the process works with an empty external Bun/JS runtime path. Keep smoke commands non-interactive and network-free.

**Tests / verification:** `bun run build:standalone --target <host-target>` followed by `bun test packages/iyon-cli/test/distribution.test.ts` and `bun run verify:standalone --artifact <path>`. Assert the executable starts, the embedded addon loads, the native smoke operation completes, the manifest checksum matches, and no separate JS runtime or unpacked addon is required. Run `cargo build --release -p iyon-native` and the relevant Bun typecheck before committing.

**Must not:** Make `crates/iyon` the release binary, download code at runtime, require a global Bun install, or add an alternate Rust-owned packaging path.

### Commit 5 — Add cross-target distribution CI and release checks

**Why:** Local packaging only proves one host. T12 requires build/test coverage and embedded-addon verification for every supported target, with failures visible before release artifacts are published.

**Files:** `.github/workflows/ci.yml`, `.github/workflows/standalone.yml`, `.github/workflows/release.yml`, `.github/actions/setup-bun/action.yml`, `scripts/release/verify-artifact.ts`, `scripts/release/target-smoke.ts`, `scripts/release/checksums.ts`

**Work:**

1. Add a Rust job for `cargo fmt --check`, workspace build/test, and the native crate’s target-specific release build. Add a Bun job for frozen workspace install, package typecheck, API-surface parity (`missing=0`, `stale=0`), runtime tests, and the existing TUI/public-app behavioral suite.
2. Add a standalone matrix over the declared supported targets. Each job builds the matching native addon, compiles the Bun executable, runs it on the host where possible, and otherwise uses the repository’s target smoke/inspection path to verify the expected embedded N-API payload and platform metadata. Native addon loading must be tested on each platform runner, not inferred from a successful TypeScript compile.
3. Verify artifacts structurally and behaviorally: executable naming/permissions, target metadata, embedded Bun marker, embedded `.node` addon identity, checksums, `--version`, and the offline native smoke operation. Fail if the artifact expects a separate JS runtime or leaves the addon as an undistributed sidecar.
4. Make release publication depend on all matrix jobs and attach only the verified executable plus manifest/checksum files. Keep release triggers and signing hooks declarative; no website deployment or package registry mutation belongs here.

**Tests / verification:** Run the workflow’s equivalent local commands for the host target: `cargo test --workspace`, `bun install --frozen-lockfile`, `bun test`, `bun run api-surface:check`, `bun run build:standalone`, and `bun run verify:standalone`. Inspect the generated CI matrix and assert every declared target has both a native build step and an addon-load smoke step before release publication is reachable.

**Must not:** Mark an artifact green from metadata alone, skip a platform because it is inconvenient, publish an unverified sidecar addon, or deploy iyon.dev.

### Commit 6 — Implement the static `iyon.catalog` index pipeline

**Why:** A future iyon.dev catalog needs discoverable package metadata without executing arbitrary package code. The contract must be deterministic, typed, provenance-aware, and able to classify apps, tools, providers, and agents from manifests alone.

**Files:** `packages/iyon-plugins/src/catalog/types.ts`, `packages/iyon-plugins/src/catalog/read-manifest.ts`, `packages/iyon-plugins/src/catalog/build.ts`, `packages/iyon-plugins/src/catalog/index.ts`, `packages/iyon-plugins/test/catalog.test.ts`, `tools/catalog/build.ts`, `tools/catalog/validate.ts`, `tools/catalog/fixtures/*`, `catalog/iyon.catalog.json`, `package.json`

**Work:**

1. Define the versioned `IyonCatalog`/`CatalogRecord` schema with package name/version, description, keywords, repository/license links when present, manifest provenance, extension entrypoint metadata, and a normalized classification set drawn from `apps`, `tools`, `providers`, and `agents`. Preserve multiple contribution kinds when a package provides more than one; do not force one package into one plugin type.
2. Read `package.json` and the `iyon` manifest field from workspace packages and configured catalog inputs using filesystem JSON parsing only. Validate names, versions, IDs, entrypoint shape, contribution metadata, and permissions declarations without resolving or importing entrypoints, running lifecycle scripts, evaluating configuration, or installing third-party code.
3. Make output deterministic: normalize paths to manifest references, sort packages/contributions/keywords, use a declared schema version, and reject malformed/ambiguous metadata rather than guessing. Include bundled extensions as ordinary catalog records so the examples and built-in product use the same contract.
4. Provide a CLI/library split: `tools/catalog/build.ts` scans inputs and writes `iyon.catalog`; `packages/iyon-plugins` exports types and validation for a future site or package manager. Add a validation command that can consume a generated catalog without executing any package JavaScript.
5. Add a malicious fixture whose entrypoint would mutate a sentinel if executed. Assert catalog generation leaves the sentinel untouched and only reads its manifest. Test missing metadata, invalid kinds, duplicate package/version records, multiple contribution kinds, deterministic output, and bundled package discovery.

**Tests / verification:** `bun test packages/iyon-plugins/test/catalog.test.ts`; run `bun run catalog:build --check` and `bun run catalog:validate catalog/iyon.catalog.json`. Assert the generated catalog is schema-valid, reproducible byte-for-byte, correctly classifies apps/tools/providers/agents, includes package/manifest provenance, and never imports or executes fixture JS. Review the generated diff for only metadata changes.

**Must not:** Build the website, execute package JS, infer capabilities from arbitrary code, create a custom npm ecosystem, or add a security promise that the normal Bun runtime does not provide.

### Commit 7 — Document extension authoring, installation, and security

**Why:** Ecosystem readiness requires a single author path that demonstrates the real public APIs. Documentation must make bundled extensions canonical examples and must explain the actual trust boundary before users install third-party code.

**Files:** `docs/README.md`, `docs/create-extension.md`, `docs/package-manifest.md`, `docs/contributions/tool.md`, `docs/contributions/provider.md`, `docs/contributions/agent.md`, `docs/contributions/app.md`, `docs/scene-extensions.md`, `docs/low-level-core.md`, `docs/tui.md`, `docs/install.md`, `docs/security.md`, `packages/iyon-sdk/README.md`, `packages/iyon-plugins/README.md`, `packages/iyon-cli/README.md`

**Work:**

1. Document the package/extension/contribution distinction, the minimal package manifest, entrypoint exports, activation/disposal, duplicate IDs, explicit replacement, provenance/errors, and how to test an extension locally. Use canonical imports `iyon:api`, `iyon:core`, `iyon:tui`, and `iyon:plugins`, with `@iyon/sdk` only for development/types.
2. Write one focused guide each for tool, provider, agent, and app contributions. Show the low-level escape hatch from one arbitrary extension, then show how contribution metadata lets the default app discover the same extension without default-app edits. Include `read` replacement, provider/model discovery, custom agent loops, and third-party app selection examples from `plugins/`.
3. Document Scene extensions as a separate axis from app registration: an extension may replace the selected app’s Scene without registering another app. Explain when to use native core operations, native handles, streams, History, TextInput, and TUI semantic values; explicitly warn against duplicating terminal/layout logic in TypeScript.
4. Document installation from npm/workspaces and the Bun standalone `iyon` executable, including supported targets and the fact that no separate JS runtime is needed for the shipped binary. Link to the static catalog contract without presenting the future site as deployed.
5. Document the security model plainly: installing an extension grants normal Bun/npm code authority; manifest metadata is discovery, not a sandbox; tool approvals and core filesystem/workspace policies still apply where they already exist; provenance makes failures diagnosable; users should review dependencies and package source. Do not claim mandatory network/filesystem isolation.
6. Make bundled extensions under `plugins/app/iyon`, `plugins/agents/iyon`, `plugins/providers/*`, and `plugins/tools/*` the runnable examples. Keep examples on the same loader and public paths as third-party packages.

**Tests / verification:** Run the repository’s Markdown/link/code-example check (the command established by the docs package), `bun run catalog:validate`, and the SDK/package typecheck. Assert every required topic is linked from `docs/README.md`, examples import only canonical public paths, no guide references WASM/Rhai/WIT, and the install guide describes the standalone executable rather than a separate JS runtime.

**Must not:** Add a new API solely for documentation, describe deprecated Rust application internals as the extension contract, promise a sandbox, or make the docs depend on a deployed website.

### Commit 8 — Add the full architectural acceptance matrix

**Why:** The tranche is complete only when the bundled product and arbitrary third-party TypeScript are proven equivalent at the public boundaries. These tests turn every required acceptance statement into a machine-checked fixture and protect the later breaking release from silently restoring special cases.

**Files:** `tests/acceptance/arbitrary-extension.test.ts`, `tests/acceptance/tool-parity.test.ts`, `tests/acceptance/provider-parity.test.ts`, `tests/acceptance/agent-parity.test.ts`, `tests/acceptance/app-parity.test.ts`, `tests/acceptance/scene-power.test.ts`, `tests/acceptance/rust-consumer.test.ts`, `tests/acceptance/typescript-surface.test.ts`, `tests/acceptance/tui-behavior.test.ts`, `tests/acceptance/distribution.test.ts`, `tests/fixtures/acceptance/arbitrary-extension/*`, `tests/fixtures/acceptance/custom-tool/*`, `tests/fixtures/acceptance/custom-provider/*`, `tests/fixtures/acceptance/custom-agent/*`, `tests/fixtures/acceptance/custom-app/*`, `tests/fixtures/acceptance/scene-extension/*`, `tests/fixtures/rust-consumer/*`, `tools/api-surface/`, `crates/iyon-native/tests/acceptance.rs`

**Work:**

1. Create fixture packages that use only public paths. The arbitrary-extension fixture must, from one entrypoint, use an npm dependency plus filesystem/process/network access, register a tool and command, subscribe to session events, and replace a Scene. Use local deterministic process/network doubles so the test is offline and repeatable.
2. Verify builtin tool parity by replacing `read` through the extension registry and asserting the default app discovers and executes the replacement without a default-app source change. Verify provider parity by registering a provider and asserting the default app’s provider/model metadata and UI discover it without a provider-name branch.
3. Verify agent parity with a third-party loop using the same kernel operations, tool/approval/event APIs, and cancellation contract; verify app parity by selecting a third-party base app and asserting the default Iyon app is not instantiated. Verify Scene power by replacing the selected app’s Scene without registering a new app.
4. Add a pure Rust consumer fixture that depends on `iyon-api`, `iyon-core`, and `iyon-tui` and builds/tests without Bun. Add a machine-tracked TS surface assertion that runs `tools/api-surface` and fails on any missing or stale reachable path. Keep `crates/iyon` compatibility tests in the matrix until the later breaking release.
5. Re-run the TUI behavioral oracle through the new Bun/native harness: native scrollback, streaming smoothness, history promotion, composer placement, paste, approval update, cancellation, and shutdown with outstanding work. Preserve the existing public-app assertions rather than snapshotting a second TS renderer.
6. Execute the distribution fixture against a built standalone artifact and assert it contains Bun plus the native addon, loads without a separate JS runtime, and can perform one core and one TUI/native-handle operation. Make the acceptance report list each of the ten required statements and the exact fixture/test that proves it.

**Tests / verification:** `bun test tests/acceptance`; `cargo test --workspace`; `bun run api-surface:check`; and the host-target standalone build/verify commands from Commits 4–5. Require all ten acceptance rows to pass: arbitrary extension power, builtin tool parity, provider parity, agent parity, app parity, Scene power, pure Rust API, tracked TypeScript API, TUI behavior, and distribution. The final report must show zero missing/stale API entries, zero leaked extension/native owners, and a standalone executable that loads its embedded addon.

**Must not:** Add private test-only privileges, weaken the public API to satisfy a fixture, delete compatibility crates/providers, replace the native TUI with a JSON/TypeScript clone, or make acceptance pass by skipping unsupported targets.

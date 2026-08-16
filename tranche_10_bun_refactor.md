# Tranche 10 — Migrate the default application and CLI

## Mission

Make the shipped Iyon product Bun/TypeScript-owned: port the Rust product app’s conversation state, composer, event projection, streaming display, tool lifecycle, approvals, status/footer, reasoning controls, and exit behavior into `plugins/app/iyon/`, then complete `packages/iyon-cli/` so Bun loads configuration and the native addon, discovers and activates the same extensions used by third parties, selects the provider/agent/app, runs the default app, and cleans up the terminal/runtime. Preserve the existing Rust app behavior as the oracle during overlap, make the Bun standalone executable the canonical `iyon`, remove the Rust product `main.rs`, and retain `crates/iyon` only as a deprecated library compatibility layer.

## Prerequisites

- T0 has replaced the old architecture document and frozen the migration invariants.
- T1 has established the Bun workspace, `iyon-native` addon loading/embedding path, virtual modules, and a standalone Bun smoke executable.
- T2 has produced `tools/api-surface/` coverage with no missing or stale reachable Rust capability.
- T3 has made `iyon-core` a provider-agnostic kernel while preserving the compatibility surface; the product path must not call `register_builtin_defaults`.
- T4 exposes the supported `iyon:api` and `iyon:core` TypeScript façades, including model selection, core commands/events, reasoning levels, and cancellation.
- T5A–T5C expose the generic value/handle TUI bindings, including `App`, `AppCx`, `View`, `History`, stream handles, `TextInput`, `ScrollPane`, components, key routing, timers, and typed outputs. T5D provides the headless harness; T5E provides the TypeScript component/trait path.
- T6 provides one transactional extension/package loader for bundled and third-party extensions, with app/agent/provider/tool contribution discovery and activation.
- T7 has migrated the bundled providers and their provider/auth contributions to TypeScript.
- T8 has migrated the bundled tools and generic tool lifecycle events to TypeScript. The app consumes those lifecycle events and never branches on a tool name for presentation.
- T9 has migrated the default agent to TypeScript and exports the same low-level agent contract used by a fixture custom agent.
- The current Rust app tests remain green before implementation begins, especially `cargo test -p iyon --test public_app` and the related `crates/iyon/src/tui/backend.rs` and `crates/iyon/src/tui/state.rs` tests.

This tranche must not assume T11’s bridge optimization or T12’s distribution hardening. It may use the T1 embedding mechanism, but it must prove the real app and real binary through that existing mechanism.

## Invariants (do not violate)

- Bun owns process control. There is no Rust `main()` that launches Bun, no embedded Bun runtime in Rust, and no Rust-owned product event loop after cutover.
- The shipped path is exactly `TS CLI → TS plugin runtime → TS default app → TS default agent → TS providers/tools → iyon-native.node → iyon-api/core/tui`.
- `iyon-native` remains a bridge, not a product/application layer. Product-specific state transitions, event mapping, tool-card lifecycle, approval behavior, composer policy, and provider selection stay in TypeScript.
- Use native generic projection, streaming, history, scrollback, input, and terminal primitives. Do not build a JSON duplicate renderer, a private-scrollback renderer, or one N-API call per fluent `View` operation. Keep sensitive smoothing/pacing and stateful history behavior in the reusable native TUI handles where T5 exposes them.
- The default app understands only generic tool lifecycle from T8: draft/prepared/started/updated/approval/result/finished/cancelled/error and generic tool metadata. It has no per-tool-name renderer or allowlist.
- Preserve `MAX_COMPOSER_ROWS = 13`; large paste means more than 1000 characters OR more than 10 lines; normalize CRLF/CR to LF and tabs to four spaces; preserve the marker/expansion behavior for large paste.
- Preserve Ctrl+C semantics: clear non-empty composer, cancel an active turn when present, otherwise render the goodbye state and exit. Escape cancels only an active turn. Preserve Shift+Tab reasoning cycling and forwarding to the agent/core.
- Preserve multiline composer borders, footer ordering/content (provider, model, effort, status), working spinner/pacing, muted steering queue, bottom positioning of history, native scrollback, frozen results, and `Goodbye` ordering on exit.
- Preserve draft identity by `(message_id, content_index)` and update a single live card when a provider/tool ID arrives late. Do not create duplicate cards for one draft.
- Preserve backend event ordering and filtering from the Rust `CoreEventMapper`: role-aware text/thinking deltas, tool construction before execution, finished-result details, cancellation cleanup, safe adjacent delta coalescing, and bridge shutdown/cancellation.
- Preserve current CLI commands and provider fallback: no arguments and `iyon run` run the app; `iyon auth login`, `iyon auth logout`, and `iyon auth status` retain their behavior. `IYON_PROVIDER` wins; otherwise prefer an available OpenRouter key, then Codex credentials, then mock. Missing selected-provider credentials fall back to mock with the current warning behavior. `IYON_MODEL` and the existing OpenRouter default remain unchanged.
- Authentication secrets, tokens, and PII must not be logged. Native async operations have explicit cancellation/ownership; dropping a Promise is not treated as cancellation.
- Do not delete public APIs of `iyon-api`, `iyon-core`, or `iyon-tui`. Keep `crates/iyon` buildable as a deprecated library compatibility layer and give it no new product features.

## Out of scope

- T11’s optimization of View command buffers, batching, or other native-bridge performance work. Record measurements needed by T11, but do not compact or redesign the bridge here.
- T12 packaging polish, release signing, installer/channel work, ecosystem documentation, and broad hardening beyond the smoke/CI checks needed to establish the canonical executable.
- Any WASM, Rhai, WIT, custom JavaScript engine, Rust-owned process model, or Rust provider HTTP layer.
- Migrating providers, bundled tools, or the default agent; those are T7–T9 inputs. Do not copy their implementations into the app or CLI.
- Moving product behavior into `iyon-native` merely to avoid porting it, adding hidden built-ins, or creating a separate bundled-extension loader.
- Removing the `crates/iyon` compatibility crate or deleting public Rust APIs. Removing only the Rust product binary target and private product-only code is part of the final cutover, subject to preserving the compatibility library contract and its regression tests.
- Rewriting `ARCHITECTURE.md`, `iyon-api.md`, `iyon-core.md`, `iyon-tui.md`, or `iyon.md`; those are migration contracts and are not T10 implementation files.

## Adjacent tranche contracts

### Exports for later tranches

T10 exports a real `plugins/app/iyon/` app contribution that is loadable through the ordinary T6 extension mechanism, consumes generic T8 tool lifecycle and T9 agent/provider contracts, and exposes the same app-selection/run contract that a third-party app can use. It exports a headless app factory/test fixture capable of consuming deterministic core events, a canonical generic frontend-event model, and a standalone Bun `iyon` executable whose embedded addon path is proven. It also leaves a deprecated Rust library path and Rust public-app oracle tests available for compatibility/regression verification.

### Consumes from earlier tranches

T10 consumes the T1 Bun/native bootstrap and embedded `.node` loading, T4 `iyon:api`/`iyon:core` surfaces, T5 native TUI values/handles/harness/traits, T6 transactional discovery and activation, T7 provider/auth contributions, T8 generic tool contributions/events, and T9 default-agent contribution. The app receives a selected agent/provider and core event stream through those contracts; it does not construct provider-specific clients or register built-in tools itself.

### Must not steal from neighbors

T10 must not take provider/tool/agent implementation from T7–T9, native rendering/layout behavior from T5 or T11, or packaging/release policy from T12. If a T11 optimization needs an interface, T10 may leave a measured seam or a typed TODO at the existing T5 boundary, but must not implement the optimization. If T12 needs a distribution hook, T10 must provide only the standalone executable and addon-embedding proof required by this tranche.

## Commits

### Commit 1 — Add the default app package shell

**Why:** Establish the bundled app as an ordinary extension before porting behavior. This makes the default app load through the same T6 package/discovery/activation path as third-party apps and provides a stable home for the later state, event, and view modules.

**Files:**

- `plugins/app/iyon/package.json`
- `plugins/app/iyon/src/index.ts`
- `plugins/app/iyon/src/app.ts`
- `plugins/app/iyon/src/contracts.ts`
- `plugins/app/iyon/test/smoke.test.ts`

**Work:**

- Declare the workspace package and its extension metadata using the existing T6 package/contribution schema. Register one app contribution with the canonical default-app ID; do not add an `is_builtin` branch or a second loader.
- Define the app-owned types that will cross the app reducer boundary: `IyonState`, `IyonAction`, `FrontendEvent`, `ToolDraftKey`, `ToolUpdatePresentation`, `InfoState`, and the generic live-tool/approval records. Keep `FrontendEvent` aligned with the Rust public surface (`TurnStarted`, user/assistant/thinking deltas, tool draft lifecycle, tool execution lifecycle, approval lifecycle, results, config changes, finish/fail/cancel).
- Implement `createIyonApp(dependencies)` as a typed factory. Inject the selected agent/core command surface and selected model metadata; do not read provider environment variables or instantiate a provider from inside the app.
- Wire the factory to the T5 `App` type and a minimal valid native `History`/theme configuration so the package builds and can be instantiated by the T6 loader. Keep the first view deliberately small; all user-visible parity behavior is added in later commits.
- Add a smoke test that discovers/activates the package through the ordinary loader, verifies its contribution ID and app factory, and starts/stops it with the T5D harness without a terminal subprocess.

**Tests / verification:**

- `bun test plugins/app/iyon/test/smoke.test.ts`
- `bunx tsc --noEmit -p plugins/app/iyon/tsconfig.json` (or the workspace typecheck command established by T1)
- Assert that the package is activated by the normal loader, not a bundled-only path, and that the app factory has no provider/tool-name selection inputs.
- Run `cargo test --workspace --all-targets` to keep the Rust workspace green while the new package is introduced.

**Must not:**

- Do not port Rust behavior into `iyon-native` or add a temporary Rust launcher.
- Do not add provider/tool/agent implementations or a per-tool renderer.
- Do not edit the Rust binary or delete any old tests in this commit.

### Commit 2 — Port composer, conversation state, and native layout

**Why:** Move the app’s product state and static composition while the app can still be driven entirely by deterministic actions. This isolates composer policy and history/layout behavior from the later backend-event bridge.

**Files:**

- `plugins/app/iyon/src/app.ts`
- `plugins/app/iyon/src/state.ts`
- `plugins/app/iyon/src/composer.ts`
- `plugins/app/iyon/src/view.ts`
- `plugins/app/iyon/src/theme.ts`
- `plugins/app/iyon/test/composer.test.ts`
- `plugins/app/iyon/test/state.test.ts`

**Work:**

- Port the responsibilities of Rust `IyonState::init`, `IyonState::view`, `footer_text`, `clear_composer`, and the conversation/user-batch bookkeeping into explicit TypeScript reducer/state functions. Keep component handles and native stateful objects as handles; do not serialize `History`, stream, or input state into a TS JSON model.
- Build the native composition with `TextInput`, `History`, `HistoryStream`/stream handles, `ScrollPane`, `View.component`, native insets/borders, and the existing theme API. Configure history bottom layout equivalent to `HistoryLayout::from_parts(Insets(0, 0, 1, 0), 1)` and keep the composer/footer resident below the history.
- Implement a multiline composer with the exact `MAX_COMPOSER_ROWS` and border edges. Route submit, paste, Ctrl+C, Escape, and Shift+Tab through typed actions and native key/paste routing rather than polling terminal input in TS.
- Port `ComposerPasteStore` exactly: LF/tab normalization, the two large-paste thresholds, collision-safe markers, expansion on submit, and clearing after submission/cancellation.
- Model `InfoState` and footer formatting so provider, model, effort, and status remain independently updateable and ordered as before. Keep reasoning levels typed through `iyon:api`/`iyon:core` rather than duplicating provider-specific levels.
- Port user batches, working/steering state, and the state flags needed for later live streams, but keep actual backend event handling behind the typed reducer interface introduced in Commit 1.

**Tests / verification:**

- `bun test plugins/app/iyon/test/composer.test.ts plugins/app/iyon/test/state.test.ts`
- Exercise CRLF, lone CR, tabs, exactly-at-threshold and over-threshold paste, marker collisions, submit expansion, and paste-store clearing.
- Use T5D at multiple viewport sizes to assert a 13-row maximum, multiline borders, footer presence/order, composer bottom placement, and history rows remaining resident above the composer.
- `bunx tsc --noEmit` and `cargo test --workspace --all-targets`.

**Must not:**

- Do not implement backend/provider selection here.
- Do not reproduce native wrapping, projection, scrollback, or layout algorithms in TypeScript; compose and configure the native primitives.
- Do not change the Rust app implementation or its oracle tests yet.

### Commit 3 — Port core-event mapping and streaming/tool lifecycle

**Why:** Reproduce the behavior-sensitive event boundary before adding the final interaction polish. This is where late tool IDs, assistant/reasoning smoothing, result details, and bridge cancellation can regress if the Rust mapping is translated loosely.

**Files:**

- `plugins/app/iyon/src/backend.ts`
- `plugins/app/iyon/src/streaming.ts`
- `plugins/app/iyon/src/tool-cards.ts`
- `plugins/app/iyon/src/state.ts`
- `plugins/app/iyon/src/app.ts`
- `plugins/app/iyon/test/backend.test.ts`
- `plugins/app/iyon/test/streaming.test.ts`
- `plugins/app/iyon/test/tool-cards.test.ts`

**Work:**

- Port `CoreEventMapper` from `crates/iyon/src/tui/backend.rs` as a pure, typed mapper. Track message roles by message ID; ignore agent/message events that are not UI events; map text/thinking deltas only for the correct role; lower generic tool updates; preserve tool-result details from the authoritative finished-result event; clear transient message state on cancellation; and coalesce only adjacent assistant or thinking deltas of the same kind.
- Add a native-backed bridge task that receives the T4 core event stream, maps events, sends app actions through the T5 `AppHandle`, and has an explicit `AbortSignal`/cancellation token. On app shutdown, stop the receiver before releasing the addon/runtime handle. Do not rely on a dropped Promise to cancel Rust work.
- Port the Rust `push_stream`/assistant stream path to the native stream/projection handle exposed by T5. TS chooses semantic text versus thinking segments and appends content; native owns smoothing/pacing, incremental projection, history transfer, and scrollback.
- Implement generic live tool cards keyed first by `ToolDraftKey` and secondarily by provider tool-call ID. `ToolCallPreparing` creates one draft card, arguments update it, `ToolCallPrepared` fills metadata, and a late ID reindexes the same card. Execution events update the same card; result/failure/cancel freezes it once, with no orphan result row.
- Keep the app’s tool-card data generic (`toolName`, arguments, status, text/progress/details, error flag). The view may show a generic title/status and generic structured details, but may not switch on `bash`, `read`, `edit`, or any other tool name.
- Preserve working spinner timing and muted steering queue as state transitions around native stream/tool handles, not as a second renderer or timer loop.

**Tests / verification:**

- `bun test plugins/app/iyon/test/backend.test.ts plugins/app/iyon/test/streaming.test.ts plugins/app/iyon/test/tool-cards.test.ts`
- Port the Rust mapper assertions: role/order preservation, tool construction before execution, safe coalescing boundaries, cancellation cleanup, and finished-event details.
- Assert late provider IDs reuse one draft card, adjacent drafts remain separate, tool updates do not replace the working row incorrectly, and cancellation/failure freezes without reporting success.
- Drive the bridge with a deterministic T5D event source, then abort it while idle and while streaming; assert no unhandled rejection and no event reaches a disposed app.
- `bunx tsc --noEmit` and `cargo test --workspace --all-targets`.

**Must not:**

- Do not add per-tool-name presentation or move T8 tool behavior into this package.
- Do not implement a TS markdown/layout/scrollback engine.
- Do not optimize command batching or compact View buffers; leave that to T11.

### Commit 4 — Complete approvals, interactions, and exit UX

**Why:** Finish the state machine that users directly control, including the subtle distinction between clearing composer text, cancelling work, and exiting. This makes the app behavior complete before the parity suite is introduced.

**Files:**

- `plugins/app/iyon/src/actions.ts`
- `plugins/app/iyon/src/approvals.ts`
- `plugins/app/iyon/src/state.ts`
- `plugins/app/iyon/src/view.ts`
- `plugins/app/iyon/src/app.ts`
- `plugins/app/iyon/test/interactions.test.ts`
- `plugins/app/iyon/test/approvals.test.ts`

**Work:**

- Port the Rust `update` action semantics as a flat reducer: submit expands paste and sends a core command; approval actions send approve/reject commands; Ctrl+C follows composer → active-turn → goodbye/exit precedence; Escape and explicit interrupt cancel active work; RequestExit flushes buffered assistant content before goodbye; Shift+Tab cycles the typed reasoning level and sends the change to the agent/core.
- Port approval state and routes. A pending approval freezes the relevant user batch/card arrangement, renders a generic approval prompt, and resolves the existing card in place on approval/rejection. Preserve approval IDs and tool-call IDs independently.
- Port turn-finalization rules: seal assistant streams before a steered user message, force-finalize missing tool results at turn end, freeze completed/failed/cancelled tool cards, remove working rows only at the same lifecycle boundaries as Rust, and preserve assistant text before `Goodbye`.
- Add the status/footer and reasoning style state to the final view, preserving the existing theme keys and the native `style_state` path. Keep spinner/pacing driven by the native tick/stream primitive where available.
- Add explicit app cleanup that cancels the core bridge, closes/flushes native history/stream handles, restores the terminal through T5, and returns typed errors to the CLI.

**Tests / verification:**

- `bun test plugins/app/iyon/test/interactions.test.ts plugins/app/iyon/test/approvals.test.ts`
- Assert all Ctrl+C branches, Escape behavior with/without active work, Shift+Tab cycling, approval accept/reject, stream flush before goodbye, forced finalization, footer/status updates, and terminal cleanup after normal and error exits.
- Run the Rust oracle before parity migration: `cargo test -p iyon --test public_app` and the related backend/state unit tests.
- `bunx tsc --noEmit` and `cargo test --workspace --all-targets`.

**Must not:**

- Do not remove the Rust app or its public tests in this commit.
- Do not make the CLI responsible for app state or terminal rendering.
- Do not change the generic T4/T5/T6 contracts to accommodate product-only shortcuts.

### Commit 5 — Port the Rust public app oracle to the TS headless harness

**Why:** Demonstrate behavioral parity while both implementations are available. The existing public app tests cover the high-risk streaming/history/composer interactions and must remain the acceptance oracle until the cutover is safe.

**Files:**

- `plugins/app/iyon/test/public_app.test.ts`
- `plugins/app/iyon/test/public_app_fixtures.ts`
- `plugins/app/iyon/test/snapshots/**`
- `crates/iyon/tests/public_app.rs` (only if needed to keep the old oracle deterministic or to add an explicit overlap marker; no behavior weakening)

**Work:**

- Port the scenarios from `crates/iyon/tests/public_app.rs` into T5D with the same event sequences, viewport dimensions, time advancement, and assertions against both native history lines and screen lines. Keep the Rust suite running unchanged wherever possible.
- Include, at minimum, the scenarios named `iyon_app_is_drivable_through_public_tui_harness`, `pending_assistant_smoothing_flushes_before_tool_call`, `pending_assistant_smoothing_survives_turn_cancellation`, `completed_tool_keeps_composer_below_history`, `streamed_tool_draft_is_visible_before_execution_and_reuses_one_card`, `prepared_tools_keep_order_and_only_started_tool_runs`, `approval_updates_prepared_tool_card_in_place`, `cancelling_preparing_tool_freezes_cancelled_card`, `cancelling_running_tool_does_not_mark_it_finished`, `error_result_marks_prepared_card_failed_without_orphan_result_row`, `composer_collapse_keeps_streaming_assistant_contiguous`, `composer_shrink_consumes_history_slack_before_native_transfer`, `long_no_wrap_code_does_not_pin_product_history`, `missing_tool_result_is_forced_finalized_at_turn_end`, `request_exit_flushes_buffered_assistant_text_before_goodbye`, `approval_freezes_a_user_batch_delivered_after_an_existing_tool`, `steered_user_message_keeps_working_as_stream_tail`, and `pending_steer_shows_muted_queue_beside_working`.
- Port the short-viewport markdown/list/table cases as well, because they verify that the app uses native projection/scrollback and does not pin history on long NoWrap content.
- Add a parity fixture that feeds the same generic event transcript to Rust and TS where practical, and document any unavoidable harness-only observation difference without changing product behavior. Treat ordering, visible rows, native history transfer, and lifecycle counts as exact assertions.
- Keep both suites in CI until the final cutover commit. A TS pass alone is not sufficient; the Rust oracle must remain green during overlap.

**Tests / verification:**

- `bun test plugins/app/iyon/test/public_app.test.ts`
- `cargo test -p iyon --test public_app`
- `cargo test -p iyon --lib` (covers the mapper/state-related unit tests compiled with the compatibility library)
- Run the full relevant overlap command established by the workspace, for example `bun test && cargo test --workspace --all-targets`.
- Require zero unexplained parity failures at 40×12, 40×20, 60×20, and the short viewport sizes used by the Rust tests; require no native-history regression after long code blocks and no duplicated tool cards.

**Must not:**

- Do not delete or weaken the Rust oracle before this commit passes.
- Do not replace exact behavioral assertions with snapshots alone or with a visual-only smoke test.
- Do not add product behavior to the native addon to make one parity case pass.

### Commit 6 — Make the TypeScript CLI own bootstrap and auth commands

**Why:** Move process/bootstrap parsing out of Rust while keeping CLI behavior stable and making every startup stage explicit and testable.

**Files:**

- `packages/iyon-cli/package.json`
- `packages/iyon-cli/src/args.ts`
- `packages/iyon-cli/src/bootstrap.ts`
- `packages/iyon-cli/src/main.ts`
- `packages/iyon-cli/src/cleanup.ts`
- `packages/iyon-cli/src/auth.ts`
- `packages/iyon-cli/test/args.test.ts`
- `packages/iyon-cli/test/bootstrap.test.ts`
- `packages/iyon-cli/test/auth.test.ts`

**Work:**

- Replace the Rust Clap command model with a typed Bun parser whose default command is `run`, and whose subcommands are `run` plus `auth login`, `auth logout`, and `auth status`. Preserve unknown-command/error exit behavior and human-readable diagnostics.
- Implement a testable bootstrap pipeline with injected filesystem/config/native-loader/package-discovery/terminal-cleanup dependencies. The production order must be: load config → initialize `iyon-native` → initialize virtual modules → discover packages → transactionally activate extensions → select provider → select agent → select app → run app → cleanup terminal/runtime.
- Reuse the T1 addon loader and T6 activation transaction. Import `iyon:api`, `iyon:core`, `iyon:tui`, and `iyon:plugins` through the canonical virtual-module path; do not add direct internal Rust imports or a second native loader.
- Move the CLI-facing auth orchestration to TypeScript and dispatch provider-specific login/logout/status through the T7 provider/auth contribution. Preserve the current Codex PKCE/callback, credential locations/compatibility names, OpenRouter environment/keyring lookup, refresh skew, and status output through that provider-owned implementation or existing T7 façade. Keep secrets out of logs and make browser-opening, keychain, clock, HTTP, and filesystem operations injectable for tests.
- Keep the app independent of CLI parsing and auth. The CLI produces a selected provider/agent/app context and passes it to the app factory.

**Tests / verification:**

- `bun test packages/iyon-cli/test/args.test.ts packages/iyon-cli/test/bootstrap.test.ts packages/iyon-cli/test/auth.test.ts`
- Assert no-argument/`run` equivalence, all auth subcommands, stage ordering, activation rollback on failure, cleanup on success/error/abort, and that native initialization happens before extension activation.
- Assert provider fallback precedence and explicit `IYON_PROVIDER` handling for `openrouter`, `codex`/`openai`/`openai-codex`, `mock`, and unknown values; assert `IYON_MODEL` and the existing default model are passed through unchanged.
- `bunx tsc --noEmit` and `cargo test --workspace --all-targets`.

**Must not:**

- Do not instantiate a provider, agent, tool, or app by importing a bundled implementation directly; all must come from discovered/activated contributions.
- Do not create a separate “built-in” loader, a provider allowlist, or an app-specific CLI shortcut.
- Do not remove `crates/iyon/src/main.rs` yet; the old binary remains the comparison path until the real TS runner is wired.

### Commit 7 — Wire selection and run the real default product through Bun

**Why:** Connect the completed app and CLI to the actual T6/T7/T8/T9 extension graph and prove there is no first-party shortcut in the shipped path.

**Files:**

- `packages/iyon-cli/src/selection.ts`
- `packages/iyon-cli/src/runner.ts`
- `packages/iyon-cli/src/bootstrap.ts`
- `packages/iyon-cli/src/main.ts`
- `plugins/app/iyon/src/index.ts`
- `plugins/app/iyon/test/integration.test.ts`
- `packages/iyon-cli/test/runner.test.ts`

**Work:**

- Implement contribution-based provider, agent, and app selection. Preserve the current environment/config precedence, but resolve the selected provider through the activated T7 registry, the default agent through T9, and the default app through the app contribution from Commit 1.
- Connect the selected agent’s generic core event stream to the app’s Commit 3 bridge and pass the T5 native terminal/runtime context. Ensure approval commands, reasoning changes, cancellation, and cleanup flow back through the selected agent/core rather than through a Rust compatibility function.
- Add an integration fixture with a custom agent/provider/tool contribution to prove the app consumes generic lifecycle events and that the same loader can run a non-bundled extension. Keep the default app’s only tool knowledge at the generic lifecycle level.
- Add a deterministic mock run that exercises submit → user event → assistant/thinking stream → tool draft → approval → tool result → turn finish → exit, and assert the event stream reaches the TS app and native TUI handles end-to-end.
- Add a first-party-path assertion or instrumentation seam: the run must not import `crates/iyon`’s `tui::run_with_core`, call `register_builtin_defaults`, or bypass T6 activation.

**Tests / verification:**

- `bun test packages/iyon-cli/test/runner.test.ts plugins/app/iyon/test/integration.test.ts`
- Run the default app headlessly with the mock provider and assert selected IDs, generic tool lifecycle, approval round-trip, cancellation, footer metadata, and cleanup.
- Run one custom extension fixture through the same loader and app surface; assert no bundled-only code path is selected.
- `bunx tsc --noEmit`, `bun test`, and `cargo test --workspace --all-targets`.

**Must not:**

- Do not add a Rust-owned process, subprocess bridge, or alternate provider/tool/agent path.
- Do not bring the old Rust app into the new runner merely to satisfy the integration test.
- Do not take T11 batching/command-buffer work as part of wiring.

### Commit 8 — Cut over the canonical `iyon` executable

**Why:** Once the overlap oracle and end-to-end TS path pass, make the Bun standalone executable the shipped product and retire only the Rust product binary entry point while preserving the compatibility library.

**Files:**

- `packages/iyon-cli/package.json`
- `packages/iyon-cli/build.ts`
- `package.json`
- `.github/workflows/ci.yml`
- `crates/iyon/Cargo.toml`
- `crates/iyon/src/main.rs` (delete)
- `crates/iyon/src/lib.rs`
- `crates/iyon/src/tui/**/*.rs` (only private product-only cleanup; preserve compatibility exports and behavior)
- `crates/iyon/tests/public_app.rs` (retain or relocate only if required to test the deprecated library path)

**Work:**

- Add the canonical Bun compile command using the T1-supported standalone/embedded-addon mechanism. The output name is `iyon`, and the build must statically include the `iyon-native` `.node` addon through the established package import path. Do not make a Rust binary a prerequisite.
- Change the workspace/package executable metadata so `iyon` resolves to the Bun CLI. Keep `iyon run` and no-argument invocation equivalent, and keep auth commands available from the standalone output.
- Remove the `[[bin]]` product target and delete `crates/iyon/src/main.rs`. Retain `crates/iyon` as a library crate, keep its public compatibility entry points buildable, and mark/document the layer as deprecated if the existing compatibility policy provides the appropriate mechanism. Do not add new product behavior to it.
- Remove only private Rust app code that is provably unreachable from the compatibility library after cutover. Preserve the Rust public app oracle for the compatibility layer during the release transition; if its harness must move, keep equivalent assertions and continue running them.
- Add minimal CI coverage for both paths: Rust workspace build/tests, TS typecheck/tests, the app parity suite, and standalone compile plus `dist/iyon auth status`/`dist/iyon --help` smoke checks in an isolated environment. Keep network-backed login out of CI.
- Verify the compiled binary executes with the embedded addon and reaches the real TS plugin runtime/default app path under the mock provider. Record the exact build artifact and command for T12 to harden later.

**Tests / verification:**

- `test -f packages/iyon-cli/build.ts`
- `bun run build:iyon` (or the exact workspace script added here)
- `test -x dist/iyon`
- `dist/iyon --help`, `dist/iyon auth status`, and a bounded mock-provider/headless smoke run
- `bun test`
- `bunx tsc --noEmit`
- `cargo test --workspace --all-targets`
- `cargo check -p iyon` must build the library without a Rust binary target; the deprecated compatibility API must remain available.
- CI must run the Rust oracle and TS parity suite together before accepting the cutover.

**Must not:**

- Do not push, delete the compatibility crate, delete `iyon-api/core/tui` public APIs, or make release/signing/installer changes belonging to T12.
- Do not silently retain a Rust `main()` fallback or a first-party shortcut hidden behind the standalone build.
- Do not compact native View command buffers or claim optimization work is complete; T11 starts from the measurements captured here.

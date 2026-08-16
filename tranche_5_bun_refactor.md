# Tranche 5 — Full `iyon:tui` TypeScript binding

## Mission

Make `iyon:tui` a complete TypeScript-facing TUI framework for Bun applications while keeping the Rust terminal engine authoritative. This tranche introduces lazy TypeScript semantic values, opaque native handles for stateful TUI objects, Promise-based terminal operations, JS-thread-safe adapters for the few Rust-to-TypeScript callbacks that are genuinely required, and a deterministic headless harness. A TS-only fixture must be able to build and run a composer, history, streaming text, native-history promotion, focused component/input behavior, and `Scene` rendering through the same public path available to later bundled and third-party extensions.

## Prerequisites

- T0 has replaced the obsolete architecture contract and frozen the Bun + TypeScript + N-API direction.
- T1 has supplied the buildable Bun workspace, `packages/iyon-runtime`, the `packages/iyon-cli` integration seam, `crates/iyon-native`, the N-API loader/addon build, and virtual-module plumbing. T5 consumes those interfaces; it does not replace their loader or standalone-executable work.
- T2 has supplied `tools/api-surface/`, the `iyon-tui` reachability inventory, and `packages/iyon-sdk` as the type/development package. The inventory is the migration ledger for this tranche.
- T3 has supplied the provider-agnostic `iyon-core` kernel and preserved compatibility exports. T5 must not bind or depend on `iyon-core` behavior.
- T4 has supplied the `iyon:api` and `iyon:core` TypeScript surfaces and their N-API loading conventions. T5 may share the conventions, but `iyon:tui` remains independent of the core/API Rust crates.
- The current `crates/iyon-tui` public API documents (`iyon-tui.md`) and existing Rust tests are the behavioral contract. Existing public names and builder semantics remain the starting point.
- T5 must not assume T6’s extension registry, T7–T10’s migrated product extensions, T11’s compact bridge optimizer, or T12’s final packaging/hardening work.

## Invariants (do not violate)

- Rust remains responsible for terminal I/O, terminal sizing/input acquisition, layout, wrapping, paint, native scrollback, `History`, projection, streaming mechanics, `TextInput` editing state, diff rendering internals, incremental source mapping, `Smooth`, and headless terminal simulation. TypeScript owns application state, control flow, and product/extension behavior.
- A semantic value is immutable from the caller’s perspective. `const b = a.bold()` produces a logically new value and never silently mutates `a`.
- `View`, `TextContent`, style values, `Insets`, `BorderSpec`, `DiffHunk`, `DiffLine`, projection values, and other immutable semantic builders are lazy TypeScript façades. Their private recursive builder representation is materialized once per render/operation; do not emit one N-API call per fluent operation.
- Bridge IR is private implementation detail. It is not a public `iyon:tui` type and is not a JSON serialization of native TUI state.
- `History`, `TextInput`, `StreamPane`/`TextStream`, components, and terminal runtime state are native Rust objects represented in TypeScript by validated opaque handles. Never duplicate their mutable state in a TypeScript shadow model.
- Native async work owns explicit cancellation and lifetime. Promise rejection is typed; Rust panics do not unwind through Bun; borrowed N-API values are not retained across an await. Tokio workers never directly manipulate JS values.
- If Rust needs to invoke a TypeScript implementation, the dedicated adapter queues the invocation onto the Bun/JS thread and returns an owned result. This callback path is the exception, not the representation of ordinary TUI values.
- `Scene` stays generic: `{ history?: History, body: View }`. T5 exposes it as a bindable value/object that the runtime can render and later composers can pass through. Scene compose/replace semantics belong to T6.
- Native scrollback is preserved exactly. Promotion must use the existing Rust path: paint promoted rows at the top, place the cursor at the bottom, emit ordinary full-screen CRLFs so the terminal scrolls rows into native scrollback, mirror the model with `ScrollRegionUp`, and validate differential state. Never use a DEC scrolling-region substitute, private fake scrollback, or model-only row mutation.
- Rust APIs remain public and behaviorally compatible. Binding code wraps or calls the existing semantic constructors and native engine rather than moving layout, wrapping, paint, or terminal policy into TypeScript.
- The API scanner has no permanent `Ignore` or `Unsupported` disposition for reachable `iyon-tui` APIs. A lazy façade, handle façade, or explicitly JS-thread-marshalled adapter counts as coverage when it has tests.

## Out of scope

- `iyon-api` or `iyon-core` binding work owned by T4; T5 must not add a second core/API bridge.
- T6’s package/extension loader, transactional activation, contribution registry, and Scene compose/replace extension API. T5 only exports the `Scene` value/runtime boundary those features consume.
- T7 providers, T8 tools, T9 agents, and T10’s default app, CLI product UX, footer, approvals UI, goodbye screen, tool cards, or bundled-plugin migration. The T5 fixture is a framework proof, not the product app.
- T11’s compact command buffer, batching/bridge optimization, benchmark-led performance work, or an attempt to optimize the recursive bridge representation.
- T12’s final release packaging, ecosystem policy, broad security audit, and removal of the deprecated Rust compatibility crate.
- WASM, WIT, Rhai, an embedded Bun process, a Rust-owned process loop, a custom JS engine, a private-scrollback implementation, a JSON duplicate renderer, or a second privileged loader for built-ins.

## Adjacent tranche contracts

- **Consumes from earlier tranches:** T5 consumes T1’s N-API/addon and virtual-module seams, T2’s SDK and API-surface ledger, T3’s compatibility-preserving kernel state only as an independent workspace neighbor, and T4’s established Bun/native loading conventions. It does not consume a product-level agent, provider, or tool implementation.
- **Exports for later tranches:** T5 exports the complete `iyon:tui` runtime module and `@iyon/sdk` declarations, immutable semantic builders, native handle lifecycle and error contracts, the TS-owned `Tui` Promise runtime, generic bindable `Scene`, JS-thread adapter contracts, and the deterministic headless harness. T6 can pass the T5 `Scene` through extension composers; T7–T10 can use the same TUI APIs for bundled extensions and product UX; T11 can replace the recursive materialization transport behind the same façade.
- **Neighbor boundary:** T5 does not own extension registration/activation or Scene composition/replacement (T6), providers (T7), tools (T8), agents (T9), the default app/CLI/product UX (T10), bridge optimization (T11), or final packaging/hardening (T12). No later-tranche stub is required: T5’s stable `Scene` value/accessors and render boundary are the only T6 prerequisite, while composition/replacement methods remain T6-owned.

## Commits

### Commit 1 — Establish the `iyon:tui` binding contract and coverage ledger

**Why:** The existing scanner describes Rust reachability, while the new architecture needs a stable distinction between lazy values, native handles, Promise operations, and JS-thread adapters. Establish that contract before adding bindings so the implementation can be reviewed by ownership and not by raw N-API method count.

**Files:**

- `packages/iyon-runtime/src/tui/index.ts`
- `packages/iyon-runtime/src/tui/errors.ts`
- `packages/iyon-runtime/src/tui/types.ts`
- `packages/iyon-runtime/src/native.ts` (only the TUI loading/type seam; preserve T1 behavior)
- `packages/iyon-sdk/src/tui/index.d.ts`
- `packages/iyon-sdk/src/tui/*.d.ts`
- `tools/api-surface/manifests/iyon-tui.*`
- `tools/api-surface/src/**` (only the TUI disposition/validation code)
- `packages/iyon-runtime/tests/tui_surface_contract.test.ts`
- `packages/iyon-sdk/tests/tui_types.test.ts`

**Work:**

- Define the public `iyon:tui` module exports and SDK declarations without exposing bridge records. Group each reachable `iyon-tui` symbol into `value`, `handle`, `operation`, or `adapter` disposition, with the exact Rust path and the TypeScript test that proves it.
- Define shared branded identifiers and error categories for invalid handles, disposed handles, validation failures, terminal/runtime failures, projection/stream failures, and cancelled operations. Preserve Rust error context instead of collapsing failures to strings.
- Define the public shape of `Scene`, `View`, `TextContent`, styles, `History`, `TextInput`, stream objects, `Component`, `Renderer`, `Projector`, `TextVisitor`, `TextRewriter`, and `StreamingSource` sufficiently for later commits to compile. Keep adapter callback types explicit and mark callback execution as JS-thread-owned.
- Add scanner checks that fail for missing reachable APIs, stale dispositions, public bridge-IR leakage, or a handle type whose state is declared as serializable JSON. Keep the manifest generated/validated using the T2 tool rather than hand-maintaining a second API inventory.
- Add a minimal compile-only runtime/SDK test that imports `iyon:tui`, constructs type-level placeholders, and verifies that `iyon:api`/`iyon:core` are not transitively required.

**Tests / verification:**

- `cargo check --workspace`
- `bun run typecheck` (or the T1 workspace typecheck command)
- `bun test packages/iyon-runtime/tests/tui_surface_contract.test.ts packages/iyon-sdk/tests/tui_types.test.ts`
- `cargo run -p api-surface -- --crate iyon-tui --check` (exact T2 CLI spelling retained from T2)
- Assert the ledger reports no missing/stale TUI dispositions and that public declarations contain no bridge-IR or native-state JSON type.

**Must not:** Add a native export for every Rust method, change any Rust TUI behavior, implement extension activation, or make the public API depend on T6–T10 packages.

### Commit 2 — Implement lazy semantic values and one-shot View lowering

**Why:** `View` and related semantic values are the high-volume API. They need ergonomic Rust-like builder semantics without turning every `.padding()`, `.bold()`, or `.vertical()` call into an N-API round trip.

**Files:**

- `packages/iyon-runtime/src/tui/ir.ts`
- `packages/iyon-runtime/src/tui/values/view.ts`
- `packages/iyon-runtime/src/tui/values/style.ts`
- `packages/iyon-runtime/src/tui/values/geometry.ts`
- `packages/iyon-runtime/src/tui/values/text.ts`
- `packages/iyon-runtime/src/tui/materialize.ts`
- `packages/iyon-runtime/src/tui/index.ts`
- `packages/iyon-sdk/src/tui/view.d.ts`
- `packages/iyon-sdk/src/tui/style.d.ts`
- `packages/iyon-sdk/src/tui/text.d.ts`
- `crates/iyon-native/src/tui/value.rs`
- `crates/iyon-native/src/tui/mod.rs`
- `crates/iyon-tui/src/native_binding.rs` (a small public/hidden-in-docs lowering seam only if `iyon-native` cannot reach an existing public constructor)
- `packages/iyon-runtime/tests/tui_values.test.ts`
- `crates/iyon-native/tests/tui_values.rs`

**Work:**

- Build a private recursive `ViewNode`/semantic builder representation with copy-on-write or persistent updates. Cover `View.text`, styled text, spacer, horizontal/vertical composition, grid, hanging, container, clamp, width/height rules, padding, background/foreground, border, style states, text attributes, alignment, wrapping, and fill behavior with the existing natural names.
- Mirror the existing Rust entry points in `crates/iyon-tui/src/presentation/api/view.rs`, `composition.rs`, `grid.rs`, `style.rs`, and `text.rs`: lower `View::text`/`styled_text`, `View::horizontal`/`vertical`/`grid`/`hanging`, and the fluent style/layout methods into the existing Rust `View` rather than reimplementing them in JS.
- Add one native `materialize_view` operation per render boundary. It accepts the private recursive semantic value plus handle references, validates discriminants/ranges/one-cell border glyph constraints, and calls the existing Rust semantic constructors. Handle references remain references until native resolution; they are not copied into the IR.
- Keep the recursive crossing deliberately simple and inspectable for T11. Do not introduce a compact command buffer, memoizing renderer, or bridge optimizer here.
- Ensure all fluent methods return a new façade and that repeated materialization does not mutate or consume the source value.

**Tests / verification:**

- `bun test packages/iyon-runtime/tests/tui_values.test.ts`
- `cargo test -p iyon-native tui_values`
- Add assertions that `const a = View.text("x"); const b = a.bold().padding(1);` leaves `a` unchanged, that nested composition materializes in one native call, and that a malformed private node produces a typed error.
- Reuse `crates/iyon-tui/tests/public_semantic_api.rs`, `p3c_ergonomics.rs`, and layout tests as the Rust oracle; assert the same compiled rows/styles for representative text, grid, hanging, border, and wrapping cases.
- Run `cargo fmt --check`, `cargo clippy -p iyon-native -p iyon-tui --all-targets -- -D warnings`, and the workspace TypeScript typecheck.

**Must not:** Expose `ViewNode`/bridge IR, serialize native state, add a TypeScript layout engine, or make a native call for each builder operation.

### Commit 3 — Bind native stateful handles: History, input, streams, and components

**Why:** Stateful TUI objects must retain one authoritative Rust instance across renders and async operations. This commit establishes ownership, disposal, and handle resolution before the runtime loop can use them.

**Files:**

- `crates/iyon-native/src/tui/handles.rs`
- `crates/iyon-native/src/tui/history.rs`
- `crates/iyon-native/src/tui/text_input.rs`
- `crates/iyon-native/src/tui/stream.rs`
- `crates/iyon-native/src/tui/component.rs`
- `crates/iyon-native/src/tui/errors.rs`
- `crates/iyon-native/src/tui/mod.rs`
- `packages/iyon-runtime/src/tui/handles.ts`
- `packages/iyon-runtime/src/tui/history.ts`
- `packages/iyon-runtime/src/tui/text-input.ts`
- `packages/iyon-runtime/src/tui/stream.ts`
- `packages/iyon-runtime/src/tui/component.ts`
- `packages/iyon-sdk/src/tui/history.d.ts`
- `packages/iyon-sdk/src/tui/text-input.d.ts`
- `packages/iyon-sdk/src/tui/stream.d.ts`
- `packages/iyon-sdk/src/tui/component.d.ts`
- `packages/iyon-runtime/tests/tui_handles.test.ts`
- `crates/iyon-native/tests/tui_handles.rs`

**Work:**

- Implement a typed opaque handle registry with generation/disposal checks, explicit owner association, and deterministic teardown. Do not expose Rust pointers or allow a disposed handle to be reused silently.
- Wrap `History::new`, `HistoryLayout`, unit insertion/boundary operations, `push_stream`/`update_stream`/`seal_stream`, and the existing native-transfer entry points. Resolve `View` and stream/component handles only during native materialization.
- Wrap `TextInput::new`, multiline/border configuration, text/cursor access, set/clear, stable `submitted`/change outputs, and key/paste dispatch. Preserve the native editor buffer and Unicode/cursor behavior; TypeScript only receives owned snapshots/events.
- Bind `StreamPane`/`TextStream` lifecycle, refresh/seal, follow-end and scrolling operations, plus the existing projected/exact snapshot validation. Keep the `StreamingSource` implementation adapter for Commit 6 rather than inventing a serialized source state.
- Represent a component as a native retained handle with a JS adapter identity. The native registry owns mount/revision/focus identity; the JS façade never stores a second component state copy.
- Add typed conversion for all public error variants needed by the SDK and make every method reject invalid/disposed handles.

**Tests / verification:**

- `bun test packages/iyon-runtime/tests/tui_handles.test.ts`
- `cargo test -p iyon-native tui_handles`
- Exercise a single `TextInput` through insert, paste, cursor movement, submit, and multiline transitions; assert changes originate from one native object.
- Exercise history stream lifecycle and invalid tail/seal transitions; assert Rust `HistoryError` categories survive the boundary.
- Assert two views referring to one component handle observe the same native component revision and that disposal makes subsequent operations fail deterministically.
- Run the relevant Rust crate tests for `history`, `stream`, `controls/text_input`, and `component` before and after the bridge changes.

**Must not:** Mirror mutable History/TextInput/stream/component state in TS, make native state JSON-serializable, add product-specific component behavior, or bind `iyon-core`.

### Commit 4 — Bind semantic text, projections, streaming values, themes, and diffs

**Why:** The scanner’s remaining high-value surface is the semantic text/projection pipeline and diff API. These values must preserve source mapping and semantic identity while continuing to lower through the native renderer/layout pipeline.

**Files:**

- `packages/iyon-runtime/src/tui/values/text-content.ts`
- `packages/iyon-runtime/src/tui/values/annotations.ts`
- `packages/iyon-runtime/src/tui/values/projection.ts`
- `packages/iyon-runtime/src/tui/values/stream-snapshot.ts`
- `packages/iyon-runtime/src/tui/values/diff.ts`
- `packages/iyon-runtime/src/tui/values/theme.ts`
- `packages/iyon-runtime/src/tui/projectors.ts`
- `packages/iyon-sdk/src/tui/text-content.d.ts`
- `packages/iyon-sdk/src/tui/projection.d.ts`
- `packages/iyon-sdk/src/tui/diff.d.ts`
- `packages/iyon-sdk/src/tui/theme.d.ts`
- `crates/iyon-native/src/tui/text.rs`
- `crates/iyon-native/src/tui/projection.rs`
- `crates/iyon-native/src/tui/diff.rs`
- `crates/iyon-native/src/tui/mod.rs`
- `packages/iyon-runtime/tests/tui_semantic_pipeline.test.ts`
- `crates/iyon-native/tests/tui_semantic_pipeline.rs`

**Work:**

- Cover `Block`, `Inline`, `InlineContent`, `TextContent`, `RawText`, annotations, origins, marks, roles/parts/selectors, Markdown/plain projectors, render policy, and the public text walk/rewrite operations. Preserve the distinction between raw format projection, semantic rewrite, and final `TextRenderer` lowering.
- Bind `Projection`, `ProjectionBuilder`, `ProjectionSpan`, `Smooth`, stream offsets/ranges/revisions/snapshots, projected text/hanging builders, and their validation errors without flattening source coordinates to strings or bytes in JS incorrectly.
- Bind `DiffRange`, `DiffLineNumber`, `DiffLineOffset`, `DiffLine`, `DiffHunk`, and `DiffRenderer`; preserve coordinate validation and termination metadata and lower rendered diffs to ordinary `View` values.
- Bind `Theme`, `ThemeKey`, `StyleSelector`, `StyleSpec`, color/border/inset/alignment/overflow values and named semantic styles. Theme selection remains semantic; geometry stays in Rust layout.
- Extract only narrow Rust conversion helpers around existing `content/text/*`, `projection/*`, and `content/diff/*` constructors. Reuse `MarkdownProjector`, `PlainTextProjector`, `TextRenderer`, `DiffRenderer`, and validation functions; do not port parser, wrapping, source mapping, or paint logic.

**Tests / verification:**

- `bun test packages/iyon-runtime/tests/tui_semantic_pipeline.test.ts`
- `cargo test -p iyon-native tui_semantic_pipeline`
- Port representative assertions from `markdown_smoke.rs`, `markdown_incremental.rs`, `markdown_hardening.rs`, `text_origin.rs`, `projection_public.rs`, `stream_public.rs`, `diff_public.rs`, and `text_origin.rs` into cross-boundary fixtures.
- Assert Markdown/GFM origin and nested semantic identity survive incremental projection; invalid ranges/coordinates reject with typed errors; theme styles affect `style_at_text` without moving geometry.
- Run `cargo test -p iyon-tui --test markdown_smoke --test markdown_incremental --test projection_public --test diff_public --test stream_public` and the workspace TypeScript typecheck.

**Must not:** Reimplement pulldown parsing, projection/source mapping, wrapping, diff layout, or painting in TypeScript; make Theme a geometry API; or add a Rust-owned product renderer.

### Commit 5 — Add JS-thread adapters for public traits and interaction/output routing

**Why:** Most public traits can be represented as ordinary TypeScript interfaces, but native pipelines occasionally need to call a TypeScript renderer, projector, visitor, rewriter, streaming source, or component. The adapter boundary must be explicit and safe before those traits count as complete API coverage.

**Files:**

- `packages/iyon-runtime/src/tui/traits/renderer.ts`
- `packages/iyon-runtime/src/tui/traits/projector.ts`
- `packages/iyon-runtime/src/tui/traits/text-visitor.ts`
- `packages/iyon-runtime/src/tui/traits/text-rewriter.ts`
- `packages/iyon-runtime/src/tui/traits/streaming-source.ts`
- `packages/iyon-runtime/src/tui/traits/component.ts`
- `packages/iyon-runtime/src/tui/output.ts`
- `packages/iyon-runtime/src/tui/interaction.ts`
- `packages/iyon-sdk/src/tui/traits.d.ts`
- `packages/iyon-sdk/src/tui/output.d.ts`
- `packages/iyon-sdk/src/tui/interaction.d.ts`
- `crates/iyon-native/src/tui/js_adapter.rs`
- `crates/iyon-native/src/tui/output.rs`
- `crates/iyon-native/src/tui/component.rs`
- `crates/iyon-native/src/tui/mod.rs`
- `packages/iyon-runtime/tests/tui_traits.test.ts`
- `crates/iyon-native/tests/tui_traits.rs`

**Work:**

- Define semantic TS contracts corresponding to `Renderer`, `Projector`/`ProjectorExt`, `TextVisitor`, `TextRewriter`, `StreamingSource`, and `Component`. Keep visitor/rewrite results typed as semantic values and preserve projection relation/transition validation.
- Implement a JS-thread invocation queue/adapter in `iyon-native`. Native code owns the request ID, cancellation, and result lifetime; the Bun-facing side schedules the callback and returns an owned result. A Tokio worker may await an owned response but never reads or mutates a JS value directly.
- Bind `Output`, `OutputRouter`, `EventCx`, `InteractionResult`, key/paste routing, focus, modal scope, component capabilities, timers, and component revision/tick notifications. Ensure output routing rejects conflicts as Rust does and keeps callback ordering deterministic.
- Adapt a TypeScript component’s `view()` and capabilities declaration to one native retained component handle. Component callbacks may emit outputs or request actions, but cannot directly mutate native layout/terminal state.
- Implement `StreamingSource` adapter calls as snapshot/seal/advance/compact operations with explicit ownership and cancellation. Do not make an arbitrary Rust thread call into JS.

**Tests / verification:**

- `bun test packages/iyon-runtime/tests/tui_traits.test.ts`
- `cargo test -p iyon-native tui_traits`
- Assert custom TS renderer/projector/visitor/rewriter results are invoked on the JS thread, preserve semantic/source identity, and receive no borrowed native value after an await.
- Assert a custom component can declare focus, key commands, paste handling, and ticks; focus changes and output routing follow the existing `interaction`/`component` tests.
- Assert a streaming source can advance under an injected clock, seal, and reject post-seal mutation without a worker touching JS directly.
- Run `cargo test -p iyon-tui --test scene_public --test stream_public` plus the component, interaction, output, and projection unit suites.

**Must not:** Turn every Rust trait method into an independent FFI call, permit arbitrary Tokio-to-JS callbacks, add a plugin loader, or implement Scene composition/replacement.

### Commit 6 — Expose the TS-owned terminal runtime and bindable Scene

**Why:** With values and native handles in place, TypeScript can own the app loop while Rust continues to own terminal sessions, event acquisition, frame preparation, native promotion, and writes.

**Files:**

- `packages/iyon-runtime/src/tui/scene.ts`
- `packages/iyon-runtime/src/tui/tui.ts`
- `packages/iyon-runtime/src/tui/events.ts`
- `packages/iyon-runtime/src/tui/runtime.ts`
- `packages/iyon-sdk/src/tui/runtime.d.ts`
- `packages/iyon-sdk/src/tui/scene.d.ts`
- `crates/iyon-native/src/tui/runtime.rs`
- `crates/iyon-native/src/tui/scene.rs`
- `crates/iyon-native/src/tui/mod.rs`
- `packages/iyon-runtime/tests/tui_runtime.test.ts`
- `crates/iyon-native/tests/tui_runtime.rs`

**Work:**

- Define `Tui.open(options?)`, `nextEvent()`, `render(scene)`, `resize`/terminal metadata as needed by the public API, and `close()`/explicit cancellation. `nextEvent` returns owned key, paste, resize, and termination events; it never exposes a Rust event receiver.
- Define `Scene` as a generic body plus optional native `History`, with construction/accessors that later extension composers can pass through without gaining private host access. Do not add compose/replace methods here.
- Extract or wrap the existing `TerminalSession`, `SceneHost`, `prepare_frame`, event pump, and terminal presenter seams in `crates/iyon-tui/src/application`, `scene`, `terminal`, and `backend`. The native runtime operation should accept one materialized scene and invoke the existing host/presenter path.
- Make the Promise boundary explicit: terminal input and render are native operations with owner-scoped cancellation tokens; close cancels pending operations and waits for owned workers before releasing handles.
- Keep TS control flow canonical: a caller can run `while (!done) { const event = await tui.nextEvent(); update(event); await tui.render(view()); }` without Rust invoking a product callback or owning the loop.
- Ensure Scene rendering resolves component and history handles through native registries and does not copy their state.

**Tests / verification:**

- `bun test packages/iyon-runtime/tests/tui_runtime.test.ts`
- `cargo test -p iyon-native tui_runtime`
- Add a TS fixture that opens a headless/native test runtime, renders a Scene, receives a key/paste/resize event, and closes cleanly.
- Assert an aborted `nextEvent`/`render` rejects with the cancellation error, no native worker remains, and a second close is harmless/idempotent.
- Assert rendering through the bound runtime produces the same rows/styles as the existing `SceneHost`/presenter tests and that `ScrollRegionUp` remains model-only on the wire.

**Must not:** Retain a Rust-owned application loop, spawn Bun from Rust, add product state/control flow to `iyon-native`, or replace native scrollback with a private model.

### Commit 7 — Expose the deterministic headless harness

**Why:** TS applications need a stable way to test rendering and interaction without a real terminal. The harness must exercise the same Rust engine and native scrollback ledger as production, not a JavaScript renderer or fake History.

**Files:**

- `packages/iyon-runtime/src/tui/testing.ts`
- `packages/iyon-sdk/src/tui/testing.d.ts`
- `crates/iyon-native/src/tui/testing.rs`
- `crates/iyon-native/src/tui/mod.rs`
- `packages/iyon-runtime/tests/tui_harness.test.ts`
- `crates/iyon-native/tests/tui_harness.rs`
- `crates/iyon-tui/tests/**` (only additional Rust cross-boundary assertions if an existing test helper must be made reusable; do not rewrite the existing harness)

**Work:**

- Expose a TS `AppHarness`/harness factory backed by the existing `iyon_tui::testing::start`/`AppHarness`, with fixed width/height, key input, paste, one-step draining, clock advancement, resize, exit state, screen rows, native history rows, style lookup, and cell-column lookup.
- Inject the clock into the native harness rather than reading wall time. Preserve finite ready-batch semantics so self-rescheduling components cannot make a test hang.
- Reuse `compile_view_lines`, `style_at_text`, and `cell_x_of_text` behavior from `crates/iyon-tui/src/testing.rs`; expose owned snapshots only.
- Make native History assertions observable through the sink/ledger, including accepted-row acknowledgements and differential shadow state. The harness must be able to distinguish native history rows from screen rows.
- Add a TypeScript test fixture for the default framework proof: composer input, history units, live stream updates, stream sealing, focused component input, Scene render, clock advancement, and native promotion.

**Tests / verification:**

- `bun test packages/iyon-runtime/tests/tui_harness.test.ts`
- `cargo test -p iyon-native tui_harness`
- `cargo test -p iyon-tui --features test-util`
- Assert fixed dimensions, key/paste delivery, clock-controlled ticks, style lookup, and screen snapshots.
- Assert promoted rows appear in `nativeHistoryRows` in order and are not merely removed from a model array; compare native rows, shadow rows, and the ACK ledger after each render.
- Assert the TS fixture reproduces composer/history/stream/component/Scene behavior without importing any product plugin or `iyon-core` symbol.

**Must not:** Create a JS-only fake terminal, expose private physical layout types as the public TUI model, or add product UX to the harness fixture.

### Commit 8 — Close the `iyon-tui` parity loop and wire CI acceptance

**Why:** The tranche is complete only when the scanner reaches 100% and a TS-only application demonstrates the full framework path. Keep this as a separate reviewable parity/acceptance commit so missing API categories and behavioral gaps are visible rather than hidden inside binding implementation commits.

**Files:**

- `tools/api-surface/manifests/iyon-tui.*`
- `tools/api-surface/src/**` (only final TUI coverage/reporting changes)
- `packages/iyon-runtime/tests/tui_demo.test.ts`
- `packages/iyon-runtime/tests/fixtures/tui_demo.ts`
- `packages/iyon-sdk/tests/tui_public_api.test.ts`
- `.github/workflows/ci.yml` (only the T5 Bun/TUI matrix and API-coverage steps, if the T1 CI file exists)
- `package.json` / `tsconfig.json` (only scripts/path aliases required to run the T5 checks, with no dependency changes)

**Work:**

- Finish every scanner disposition for `iyon-tui.md`, including less frequently used public enums/structs, projection validation functions, text walking/rewrite APIs, application handles/timers, theme selectors, stream builders, diff values, and `testing` feature exports. Prefer a lazy value or native handle over adding a one-off N-API symbol.
- Add the TS-only framework demo fixture. It owns state and update/view control flow, uses native `TextInput`/History/stream/component handles, creates a generic `Scene`, renders it through `Tui`, and verifies composer, history, streaming text, native history promotion, focus/input, and Scene output. Keep it framework-level; do not copy the default app’s product UX.
- Add CI checks for Rust formatting/check/tests, Bun typecheck/tests, N-API addon build/load, API scanner `missing=0`/`stale=0`, and the headless native-scrollback differential assertions. Use the existing workspace scripts and lockfile; do not add dependencies.
- Record the exported contracts for T6: stable `iyon:tui` runtime/SDK imports, `Scene` pass-through value, native handle lifecycle, `Tui` Promise runtime, JS-thread adapter contract, and deterministic harness. Explicitly leave Scene composition/replacement to T6.

**Tests / verification:**

- `test -f tranche_5_bun_refactor.md`
- `cargo fmt --check`
- `cargo check --workspace`
- `cargo test --workspace --all-targets`
- `bun run typecheck`
- `bun test`
- `cargo run -p api-surface -- --crate iyon-tui --check`
- Assert the final report is `iyon-tui` coverage 100%, `missing=0`, `stale=0`, and has no permanent unsupported entries.
- Assert the TS demo passes under the native addon and the headless harness, and that the native scrollback differential tests still reject wire-level `ScrollRegionUp` output.

**Must not:** Port the default app, add extension activation, optimize the bridge, broaden the tranche into `iyon-core`, or mark the tranche complete with scanner exclusions.

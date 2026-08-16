# Tranche 0 — Replace the architecture contract and freeze migration invariants

## Mission

Replace the stale Rust/WASM architecture description with the Bun-owned
TypeScript application contract from the architecture handoff, and record the
current-to-target migration map and behavioral invariants that every later
tranche must preserve. This tranche changes documentation only: it establishes
who owns control flow, where arbitrary extensions execute, what remains native,
how the N-API membrane is shaped, how Scene replacement works, and which
existing APIs and UX behaviors are migration baselines before infrastructure or
product code moves.

## Prerequisites

- Start from the current `plan/t0-bun` worktree and the current Rust workspace:
  `crates/iyon-api`, `crates/iyon-core`, `crates/iyon-tui`, and the temporary
  `crates/iyon` application crate.
- Treat `/tmp/iyon-handoffs/architecture-handoff.md` as decisive. The current
  `ARCHITECTURE.md` is an obsolete document to replace, not an incremental
  design source.
- Consume the four existing public API inventories as read-only migration
  baselines: `iyon-api.md`, `iyon-core.md`, `iyon-tui.md`, and `iyon.md`.
  Consume the current Rust tests as the behavioral oracle, especially
  `crates/iyon/tests/public_app.rs`, the TUI application tests, native-history
  tests, projection/smoothing tests, and the assistant-stream tests.
- No earlier tranche is required. T0 must not assume that Bun, TypeScript,
  `napi-rs`, `iyon-native`, virtual modules, the API scanner, or the SDK already
  exists.

## Invariants (do not violate)

- Bun owns the process, control flow, product behavior, extension loading, and
  the final executable. There is no Rust `main()` that launches Bun as a child
  process.
- The Rust dependency graph is frozen as:

  ```text
  iyon-api   → no Iyon dependencies
      ▲
      │
  iyon-core  → iyon-api

  iyon-tui   → no iyon-core, no iyon-api

  iyon-native → iyon-api, iyon-core, iyon-tui
  ```

  `iyon-native` is an N-API bridge, not a product/application layer. Product
  policy, extension behavior, provider selection, agent behavior, tool
  registration, and app composition do not accumulate there.
- The TypeScript dependency direction is `iyon-cli → iyon-runtime`, with
  `iyon-runtime` loading `iyon-plugins` and exposing the canonical virtual
  modules `iyon:api`, `iyon:core`, `iyon:tui`, and `iyon:plugins`. `@iyon/sdk`
  supplies development-time declarations; extension authors are not required
  to install the native addon directly.
- An extension is arbitrary TypeScript running in normal Bun. A package is a
  distribution unit, an extension is an executable module, and a contribution
  is a well-known registration. Apps, agents, providers, and tools are
  contribution types, not the fundamental runtime abstraction and not separate
  privileged plugin species.
- The selected app produces a generic `Scene`; scene extensions may compose or
  replace it, and a replacement app is selected as the base app rather than
  instantiating and erasing the bundled app. `Scene` remains separate from app
  selection and from ordinary `View` composition.
- `iyon-core` is a provider-agnostic native execution kernel. The default
  Iyon agent, bundled providers, and bundled tools become TypeScript
  extensions. `iyon-api` remains the Rust model protocol during migration;
  concrete provider implementations leave the long-term API crate in the
  provider migration tranche while deprecated Rust compatibility remains until
  its planned removal.
- `iyon-tui` remains the terminal-performance center. Rust owns terminal I/O,
  layout, wrapping, paint, native `History`, stream/projection mechanics,
  `TextInput` state, and terminal sizing/input. Semantic values may use lazy TS
  façades; stateful objects use native handles; async operations use owned,
  cancellable Promise boundaries. Do not require one N-API call per fluent
  `View` builder method or duplicate layout/terminal logic in TypeScript.
- The four inventories are baselines, not disposable documentation. Public Rust
  API reachability must later be covered by `tools/api-surface/` with no
  permanent missing/unsupported category; T0 records that gate but does not
  build the scanner or SDK.
- The following behavior is frozen for migration and must be described with
  enough precision for later tests to assert it:
  - Native scrollback promotes semantic rows by painting the rows being
    promoted at the top, placing the cursor at the bottom, emitting ordinary
    full-screen CRLFs so the terminal moves rows into native scrollback, and
    mirroring the model with the corresponding scroll-up state. It must not be
    replaced with a private fake scrollback, a model-only row mutation, or a
    DEC scrolling-region shortcut.
  - Semantic `View` is resolved before geometry. Component identity and
    semantic ownership are established during resolution; geometry is a result
    of the layout pass, not the source of application semantics.
  - `Theme` supplies named colors/styles and selector variants at semantic paint
    time. It is not a geometry, sizing, wrapping, or layout-policy input.
  - Tool calls have distinct draft and execution lifecycles: preparing,
    argument deltas, prepared, approval when required, started, updates, final
    result, and finished. A draft is visible before execution, one logical tool
    card is updated in place, and cancellation/rejection cannot be represented
    as a false successful completion.
  - Streaming has ordered Start/Delta/End semantics per content stream. Start
    establishes the stream/content slot, Delta appends to that slot, and End
    seals it; content indices remain stable and no consumer may treat a
    coalesced transport delta as a semantic boundary change.
  - An in-progress tool draft is keyed by the pair
    `{ message_id, content_index }`, not by a possibly-late tool-call ID or
    tool name. Late identity fields update the existing draft.
  - The composer is multiline and remains below history. Paste normalizes CRLF
    and CR to LF and tabs to four spaces; text at or below 1000 characters and
    10 lines is inserted directly, while larger paste is stored behind a
    collision-safe `[Pasted Content N chars]` marker and expanded on submit.
    Submit expands stored markers and clears the composer/store. Ctrl-C clears
    non-empty composer text, cancels an active turn when empty, and exits only
    when there is neither text nor an active turn; Escape cancels an active turn
    and otherwise does not exit.
  - Approval is an execution gate, not a rendering event. `NeverAsk` proceeds;
    an approval request carries a stable approval ID and matching tool-call
    identity; approval starts execution, rejection produces the rejected/error
    result, and cancellation terminates the pending/running path without
    inventing a successful finished event. The existing special escalation for
    `bash` commands containing `sudo` remains part of the baseline.
  - Stream smoothing publishes only upstream-stable projection spans, never
    splits an atomic span, releases the first available span immediately, then
    advances on the configured temporal ticks using value-count weighting. The
    current default parameters (16 ms tick, spring 2, minimum 20 and maximum
    800 units/second) and seal-to-end flush behavior are migration baselines;
    stream boundaries flush pending assistant text before the next tool card.
  - Goodbye is observable and ordered: on clean request exit, or Ctrl-C with an
    empty inactive composer, freeze the user batch, seal the assistant stream,
    clear working state, terminalize live tools, append `Goodbye.`, hide the
    normal body, and then exit. Buffered assistant text must appear before the
    Goodbye row. Ctrl-C during an active turn cancels instead of performing this
    sequence.

## Out of scope

- Do not add or modify `package.json`, `bun.lock`, `tsconfig.json`, any Bun
  package, any plugin package, `crates/iyon-native`, `napi-rs`, N-API exports,
  virtual-module loaders, kernel types, or API-surface scanners.
- Do not refactor `iyon-core` into a kernel, move providers/tools/agent/app
  code, create TypeScript declarations, or port the default product. Those are
  T3–T10 responsibilities.
- Do not optimize the future TUI bridge, serialize native TUI state as JSON, or
  redesign native scrollback. T5 owns the functional binding model and T11
  owns post-parity optimization.
- Do not alter the four inventory files, Rust sources, `Cargo.toml`, CI, tests,
  or `AGENTS.md`. Do not preserve or extend the obsolete WASM/Rhai/WIT design
  in the replacement architecture document.
- Do not add a compatibility stub for a later tranche. T1’s smoke addon,
  T2’s scanner/SDK, and later kernel/binding/loader/product work are documented
  contracts only at this point.

## Adjacent tranche contracts

### Consumes from earlier work

T0 consumes only the current Rust workspace, the four inventory documents, the
existing public application/TUI tests, and the decisive architecture handoff.
There is no earlier implementation tranche to depend on.

### Exports for later work

T0 exports two documentation contracts:

- `ARCHITECTURE.md` becomes the authoritative answer for process ownership,
  extension execution, Rust ownership, the N-API membrane, dependency graphs,
  Scene replacement, registration semantics, and the final repository shape.
- `MIGRATION.md` maps every major current module to a target location, owning
  tranche, and compatibility disposition. It also identifies the four
  inventory files and frozen behavior list as migration baselines.

Later tranches may cite those documents rather than reopening these choices.
T1 consumes the process/native-boundary contract; T2 consumes the API baseline
and parity gate; T3–T10 consume the ownership and migration rows; T11 consumes
the native-scrollback/TUI performance boundary; T12 consumes the completion and
compatibility policy.

### Must not steal from neighbors

- T1 owns Bun/N-API viability and standalone smoke. T0 only documents that the
  membrane will exist.
- T2 owns `tools/api-surface/`, generated declarations, and coverage reports.
  T0 only marks the inventories as baselines.
- T3 owns kernel extraction and `IyonCore` compatibility. T0 must not invent
  `KernelSession` or alter `register_builtin_defaults`.
- T4/T5 own real `iyon:api`, `iyon:core`, and `iyon:tui` bindings. T0 must not
  specify bridge implementation code beyond the ownership rules above.
- T6 owns package discovery, transactional activation, registries, unload, and
  Scene extension execution. T0 documents the contract but adds no loader or
  fixture.
- T7–T10 own provider, tool, agent, default-app, and CLI migrations. T0 maps
  their destinations and preserves their behavior; it does not port them.
- T11 owns bridge optimization after functional parity; T12 owns packaging and
  ecosystem hardening. Neither needs a T0 stub.

## Commits

### Commit 1 — Replace the architecture foundation

**Why:** The current `ARCHITECTURE.md` describes Rust-owned layers,
`iyon-components`, and a runtime plugin design that the handoff explicitly
rejects. Later implementation work must have one authoritative ownership and
dependency model before adding infrastructure.

**Files:**

- `ARCHITECTURE.md`

**Work:**

1. Replace the file rather than editing individual obsolete sections. Open with
   the governing rule: Bun owns control flow, product behavior, and
   extensibility; Rust owns correctness-critical native state and the terminal
   engine.
2. Document the final repository shape from the handoff, including the Rust
   crates, `packages/*`, bundled `plugins/*`, and `tools/api-surface/`. Label
   `crates/iyon` as a temporary Rust compatibility library, not the final
   executable owner.
3. Add the Rust and TypeScript dependency graphs as code blocks and ownership
   tables. State that `iyon-native` depends on all three native libraries but
   contains only binding/marshalling/lifecycle code.
4. Define the Bun runtime path from TS CLI through runtime and virtual modules
   into `iyon-native.node`; document owned asynchronous operations,
   cancellation via explicit handles/tokens and `AbortSignal` bridging, typed
   errors, and the rule that borrowed N-API values cannot cross suspended Rust
   futures.
5. Define package, extension, and contribution separately. Explain that
   arbitrary TS extensions can use normal Bun APIs and that app/tool/provider/
   agent registrations are integration metadata, not execution restrictions.
6. Define the app/Scene split and the extension escape hatch: selected app →
   base Scene → ordered compose/replace scene extensions → final Scene → TUI.
   Include the generic `Scene { history?: History, body: View }` shape and the
   rule that replacement apps do not instantiate the bundled app first.
7. State the ownership of `iyon-api`, `iyon-core`, `iyon-tui`, and the future
   TS packages. Preserve the two-stage provider compatibility policy and the
   temporary `crates/iyon` compatibility policy.
8. Add explicit sections for TUI binding categories (lazy semantic values,
   native handles, native async operations), API parity, cancellation, and the
   final executable/distribution model. Link the future migration table as
   `MIGRATION.md`, which is added in Commit 2.
9. Remove all obsolete architecture claims and old layer names. Do not add a
   replacement plugin sandbox or a second extension runtime.

**Tests / verification:**

- `git diff --check`
- Review the rendered Markdown and verify it answers: who owns the process,
  where extensions run, what core owns, what TUI owns, how TS reaches Rust,
  whether Scene can be replaced, and how registrations differ.
- `rg -n -i 'WASM|Rhai|WIT|iyon-components|Rust.*launch.*Bun|Bun.*child' ARCHITECTURE.md`
  must return no obsolete architecture claims.
- `cargo check --workspace` confirms this documentation-only commit leaves the
  Rust workspace buildable.

**Must not:**

- Touch any source, manifest, inventory, test, or CI file.
- Add a package manifest, native crate, virtual-module implementation, kernel
  type, scanner, or product behavior.
- Describe a Rust-owned process, an embedded Bun process, a WASM/Rhai/WIT
  plugin system, or a one-kind-per-plugin restriction.

### Commit 2 — Add the current-to-target migration matrix

**Why:** Architecture prose alone does not tell a migrator where each existing
module goes, when it moves, or whether its old path remains as a compatibility
surface. A row-oriented migration document makes ownership and sequencing
reviewable before code changes begin.

**Files:**

- `MIGRATION.md`

**Work:**

1. Add a table with exactly these columns: `current location`, `target
   location`, `migration tranche`, and `compatibility disposition`. Group rows
   only when the glob has one owner and one disposition; split rows where a
   current module is divided between native kernel, native bridge, TS runtime,
   and bundled extensions.
2. Cover the model protocol surface:
   `crates/iyon-api/src/{client.rs,error.rs,model.rs,stream.rs}` stays the Rust
   protocol library and is exposed through `iyon:api`; concrete provider modules
   under `crates/iyon-api/src/providers/` move to
   `plugins/providers/{mock,openrouter,openai-codex}` in T7, with deprecated Rust
   types retained temporarily and removed only in a later breaking release.
3. Cover `iyon-core` by responsibility: IDs, session state, commands/events,
   cancellation, filesystem/process primitives, tool execution contracts,
   hooks/output policy, and transcript/session invariants remain or are
   extracted into the provider-agnostic `crates/iyon-core` kernel in T3;
   `crates/iyon-core/src/tools/builtin/**` moves to the matching
   `plugins/tools/{bash,read,write,edit,grep,find,ls}` packages in T8; the
   current `IyonCore` handle remains a compatibility façade while the kernel
   is extracted.
4. Cover all public `iyon-tui` domains with explicit rows: application host,
   terminal backends, native history/scrollback, components and controls,
   geometry/layout/paint, semantic text/diff, projection and `Smooth`, streams,
   Scene, interaction/output, theme, testing harness, and public re-exports.
   These remain native in `crates/iyon-tui`, gain TS value/handle/runtime paths
   through `crates/iyon-native` and `iyon:tui` in T5, and retain Rust behavior
   and public API compatibility.
5. Split the current application crate rows: `crates/iyon/src/main.rs` moves
   to `packages/iyon-cli` in T10 and the Rust binary entry is removed only
   then; `crates/iyon/src/lib.rs` remains as a deprecated compatibility library;
   auth/provider selection moves to the provider/app packages in T7/T10;
   `src/tui/backend.rs`, controller/state, components, theme, and transcript
   move to `plugins/app/iyon` in T10; `src/tui/tools/renderers/**` moves with
   the tool packages in T8 rather than remaining an app-side tool-name switch.
6. Add rows for the four inventory documents: they remain read-only migration
   baselines at the repository root and become inputs to
   `tools/api-surface/` in T2. Do not classify them as generated output.
7. Add a “compatibility rules” section: preserve public Rust paths while
   bindings land, keep `crates/iyon` until the planned deprecation/removal
   tranche, do not create hidden native default registrations, and do not
   claim a target is complete until its owning tranche exit criteria pass.
8. Add a “migration split” note for each module group whose current location is
   not its target location. In particular, record that default agent behavior
   moves to `plugins/agents/iyon` in T9 while native kernel state/lifecycle
   remains in `iyon-core`.

**Tests / verification:**

- `test -f MIGRATION.md`
- `git diff --check`
- Check that every row has a non-empty current path, target path, tranche, and
  compatibility disposition; no row says “later” without a numbered tranche.
- Check that all four inventories, every crate, `crates/iyon/src/main.rs`,
  `crates/iyon/src/tui/**`, and each builtin tool family appear in the matrix.
- `cargo check --workspace`

**Must not:**

- Move, rename, delete, or deprecate a real source file in this commit.
- Add package manifests or stub packages for the target paths.
- Collapse all of `iyon-core` into TS: native kernel state and correctness
  boundaries remain explicitly mapped.
- Treat a provider/tool/agent/app row as a permission boundary or invent a
  separate bundled loader.

### Commit 3 — Record extension, binding, Scene, and API-baseline contracts

**Why:** The extension model and the TS/Rust membrane are the highest-risk
architectural boundaries. They need a focused reviewable contract, including
what counts as API coverage, before T1/T2/T5 can implement anything.

**Files:**

- `ARCHITECTURE.md`
- `MIGRATION.md`

**Work:**

1. Add the extension lifecycle contract: discover package, parse manifest,
   validate compatibility, resolve the TS entrypoint, activate transactionally,
   validate registrations, commit, and roll back all contributions on import or
   activation failure. State duplicate-ID failure by default, explicit
   `replace: true`, layered precedence, source metadata, reverse-order
   disposal, and restoration of the previous layer on unload.
2. Describe `iyon:plugins` registries for tools, providers, agents, apps,
   commands, shortcuts, and scene extensions. State that bundled and external
   packages use the same loader and there is no `is_builtin` shortcut.
3. Define the binding contract for `iyon:api`, `iyon:core`, and `iyon:tui`:
   immutable semantic builders such as `View` may be lazy TS façades;
   stateful objects such as `History`, `TextInput`, streams, components, and
   terminal runtime use native handles; long-lived operations return owned
   Promises with explicit cancellation. Native calls are semantic boundaries,
   not a mechanically generated symbol for every Rust method.
4. Mark the exact baseline files in a dedicated table and state that their
   reachable paths, aliases, fields, variants, associated items, cfg/features,
   and re-exports are the initial parity universe for T2. The files remain
   human-readable source inventories; the scanner and generated manifest are
   later artifacts.
5. Add the Scene extension contract and the app-selection distinction to the
   migration document’s ownership notes so T6 can implement it without
   changing the meaning of an app registration.
6. Add cross-links from the architecture’s API-parity section to `MIGRATION.md`
   and from the migration rows to the relevant baseline inventory. Keep all
   wording normative (“must”, “must not”, “compatibility”) rather than
   speculative API sketches.

**Tests / verification:**

- `git diff --check`
- `rg -n 'iyon:api|iyon:core|iyon:tui|iyon:plugins|replace: true|MIGRATION.md' ARCHITECTURE.md MIGRATION.md`
- Verify each baseline is named exactly once as an input/baseline and is not
  listed as generated or disposable.
- Verify the documents state that arbitrary TS extensions may use ordinary Bun
  filesystem/process/network APIs and that registration type is integration
  metadata, not sandboxing.
- `cargo check --workspace`

**Must not:**

- Implement the loader, registry, N-API bindings, SDK declarations, scanner,
  or any fixture extension.
- Add one-plugin-one-kind rules, provider/tool allowlists, fake permissions, or
  a second activation path for built-ins.
- Change any public API inventory or promise literal 1:1 N-API coverage.

### Commit 4 — Freeze the behavioral invariant ledger

**Why:** Later ports can otherwise preserve names while changing scrollback,
  stream ordering, composer behavior, approvals, or shutdown semantics. This
  commit turns the current implementation and test oracle into explicit
  migration acceptance rules.

**Files:**

- `ARCHITECTURE.md`
- `MIGRATION.md`

**Work:**

1. Add a “Frozen migration behavior” section to `ARCHITECTURE.md` with one
   subsection per required invariant: native scrollback algorithm, semantic
   View-before-geometry, Theme-not-geometry, tool lifecycle, streaming
   Start/Delta/End, draft key, composer, approvals, smoothing, and Goodbye.
2. For native scrollback, document the physical promotion transaction and
   point to `crates/iyon-tui/src/history/native/mod.rs`, the `NativeHistorySink`
   seam, and the application host’s pressure-drain path as the current oracle.
3. For semantic/layout rules, point to `crates/iyon-tui/src/scene/root.rs`, the
   presentation/layout modules, and theme modules; make clear that Theme
   resolution cannot feed geometry.
4. For event/lifecycle rules, map the current
   `ModelStreamEvent`, `CoreEvent`, `FrontendEvent`, and
   `ToolDraftKey` paths. Record the draft identity pair and the distinction
   between draft preparation, execution, approval, result, and terminalization.
5. For composer and shutdown, record the current thresholds and action matrix
   from `crates/iyon/src/tui/state.rs` and `crates/iyon/src/tui/mod.rs`, including
   marker expansion, composer clearing, Ctrl-C/Escape behavior, body hiding,
   and the ordered `Goodbye.` row.
6. For smoothing, record the current `SmoothConfig` defaults, stable-span and
   atomic-span rules, first-span immediate release, timed release, and sealing;
   identify the assistant-stream tests and public app tests that later ports
   must replay.
7. Add a compact behavior-oracle table in `MIGRATION.md` with columns
   `invariant`, `current source/test oracle`, `future owner`, and
   `compatibility requirement`. Include the relevant named tests (for example
   `streamed_tool_draft_is_visible_before_execution_and_reuses_one_card`,
   `pending_assistant_smoothing_flushes_before_tool_call`,
   `request_exit_flushes_buffered_assistant_text_before_goodbye`, and the
   native-history/projection test modules).
8. State that T0 freezes behavior but does not add characterization tests or
   alter runtime code; later tranches may add tests only as part of their
   implementation and must preserve the ledger.

**Tests / verification:**

- `git diff --check`
- Check that all ten required invariant labels are present and each has a
  current source/test oracle plus a future owner.
- `rg -n 'native scrollback|View.*geometry|Theme.*geometry|Start.*Delta.*End|message_id|content_index|Goodbye|smoothing|approval' ARCHITECTURE.md MIGRATION.md`
- `cargo check --workspace`
- `cargo test --workspace --no-run` establishes that the existing Rust test
  targets still compile without a runtime change.

**Must not:**

- Change thresholds, event variants, terminal operations, stream algorithms,
  action handling, or test assertions.
- Replace exact behavior with vague “UX parity” language, or make a later
  TypeScript implementation owner responsible for native layout/scrollback.
- Add new characterization tests in T0.

### Commit 5 — Cross-document review and T0 exit gate

**Why:** The tranche exits only when a fresh engineer can use the documents as a
complete contract and later tranches can identify their inputs and boundaries
without reopening architecture decisions.

**Files:**

- `ARCHITECTURE.md`
- `MIGRATION.md`

**Work:**

1. Add a short “T0 exit gate” to `ARCHITECTURE.md` linking the migration matrix,
   baseline inventories, frozen behavior ledger, and numbered tranche owners.
2. Resolve terminology drift across both documents: use `iyon-native` only for
   the bridge, `iyon-core` only for the native kernel, “extension” for arbitrary
   executable TS, and “contribution” for registration metadata. Remove any
   accidental implication that apps/tools/providers/agents are isolated plugin
   runtimes.
3. Check every migration row against the final repository shape and every
   neighbor boundary against the handoff sequence T0→T12. Add no new design
   choice during this editorial pass.
4. Confirm the final documents answer all seven exit questions and explicitly
   state: no code migration before T0 is merged; no T1/T2/T3/T4/T5/T6 stub is
   required; and T0 leaves Rust behavior untouched.

**Tests / verification:**

- `git diff --check`
- `test -f ARCHITECTURE.md && test -f MIGRATION.md`
- `cargo fmt --all -- --check`
- `cargo check --workspace`
- `cargo test --workspace --no-run`
- Manually verify the seven exit questions from the tranche brief against the
  final rendered `ARCHITECTURE.md` without opening source code.
- Verify `git status --short` contains only the intended documentation changes
  for this future T0 series.

**Must not:**

- Add a README pointer that becomes a new architecture source of truth, edit
  the four inventories, or modify any Rust/TS/build/test file.
- Expand T0 into implementation, API generation, migration, or performance
  work merely because a cross-document inconsistency is discovered; resolve
  only documentation wording and defer implementation questions to the owning
  tranche.


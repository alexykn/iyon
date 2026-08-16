# Tranche 8 — Migrate bundled tools to Bun extensions

## Mission

Make `bash`, `read`, `write`, `edit`, `grep`, `find`, and `ls` ordinary TypeScript tool extensions loaded through the same package and extension registration path as third-party packages. Each contribution must own its input contract, execution behavior, approval metadata, call rendering, and result rendering, while the generic runtime owns only tool lifecycle and transcript plumbing. The Bun path must drive native `ToolExecution` state through the existing `iyon:core`/`iyon-native` APIs; the Rust builtin executors and the Rust application’s per-tool renderer table may remain only as compatibility scaffolding during the migration and must not remain on the product path at tranche exit.

## Prerequisites

- T0 has replaced the old architecture document with the Bun + TypeScript + N-API architecture and frozen the migration invariants.
- T1–T5 provide the Bun workspace, `iyon-native` addon, `iyon:api`/`iyon:core`/`iyon:tui` façades, native tool-execution handles, canonical transcript values, and semantic `View` construction. T8 consumes those public surfaces; it does not add a second native bridge.
- T6 provides transactional package activation, duplicate-ID protection, explicit layered replacement, hooks, and one loader for bundled and third-party extensions. T8 must use that loader rather than adding a builtin registry or `isBuiltin` branch.
- T7 has moved bundled providers to TypeScript and supplies the Bun-owned runtime harness used to load and invoke contributions. T8 must not depend on the future default-agent implementation from T9 or the default-app/CLI port from T10.
- The current Rust implementations are the behavioral oracle: `crates/iyon-core/src/tools/builtin/{bash,read,write,edit,grep,find,ls}.rs`, `crates/iyon-core/src/tools/output.rs`, `crates/iyon-core/src/tools/process.rs`, `crates/iyon-core/src/fs/{read,workspace}.rs`, and the renderer behavior in `crates/iyon/src/tui/tools/renderers/`. Preserve their schemas, validation, cancellation, path policy, truncation, details, diff, and error semantics unless a prior tranche has already made the equivalent public contract more precise.

This tranche consumes the earlier tranches’ public native lifecycle, workspace, process, mutation-queue, hook, transcript, and TUI APIs. It exports seven independently loadable tool packages, a typed `defineTool` contract, bundled-tool registration data, generic lifecycle dispatch, and a fixture proving atomic replacement of one complete contribution. T9 may consume those tools and lifecycle APIs for the default agent; T10 may consume the same registration and rendering path for the default app. T8 must not steal agent-round policy, subagent orchestration, default-app ownership, CLI startup, provider behavior, or post-parity native bridge optimization from its neighbors.

## Invariants (do not violate)

- Bun owns tool selection, hooks, approval policy, execution, and continuation. Rust owns native state transitions and terminal correctness; do not add WASM, Rhai, WIT, a Rust-owned process, or a Rust subprocess that launches Bun.
- The lifecycle remains canonical: model call → registered TS tool selection → native `ToolExecution` creation → before-tool hooks → native approval transition → `tool.execute()` → native updates → after-tool hooks → native finish/fail → canonical transcript result. A thrown execution error, rejected approval, hook failure, cancellation, or unknown tool must produce the real failure/cancellation path, never a fabricated success.
- `defineTool` contributions contain both execution and presentation semantics. `renderCall` and `renderResult` receive generic lifecycle values and return semantic `View` values through `iyon:tui`; they must not perform terminal writes, scrollback commits, wrapping, or physical layout.
- The default app/runtime contains no per-tool presentation branches such as `toolName === "bash"`, `toolName === "read"`, or `toolName === "edit"`. Unknown tools always use the generic renderer. Tool-specific formatting belongs in the contribution that executes the tool.
- Bundled tools may call safe native workspace and mutation helpers to retain current behavior, but those helpers are convenience APIs, not an allowlist or the only filesystem/process capability available to third-party TypeScript extensions.
- Bash approval, including the existing sudo-sensitive behavior, is expressed by the bash contribution or the product tool-policy contribution. No generic lifecycle function may inspect a tool name to special-case bash or sudo.
- Duplicate IDs continue to fail by default. `replace: true` replaces the complete contribution atomically: execution, schema/metadata, `renderCall`, and `renderResult` change as one registration layer.
- The seven bundled tools retain their current public names and tool schemas: `bash`, `read`, `write`, `edit`, `grep`, `find`, and `ls`. Preserve the existing result text/details shape, model-output limits, `fullOutputPath`, `diff`, truncation notices, relative POSIX paths, directory suffixes, line-ending/BOM behavior, fallback from `rg`/`fd`, and cancellation checks.
- The Rust compatibility crate and builtin executor modules may continue to compile and be directly unit-tested while T8 lands, but no Bun product path may call `iyon-core/src/tools/builtin/*` after the final commit. T3’s kernel rule still applies: the kernel must not call `register_builtin_defaults`.

## Out of scope

- T9’s default agent migration, tool-round policy, 16-tool limit, subagents, or agent-specific prompt policy. T8 only supplies registered tools and the generic lifecycle they need.
- T10’s default app and CLI migration, startup/auth UX, removal of `crates/iyon/src/main.rs`, or final Rust compatibility-crate deprecation. T8 removes Rust per-tool presentation so T10 cannot copy it, but does not port the whole app.
- T7 provider changes, model transport, authentication, or provider-specific tool policy.
- T11 bridge batching/optimization and T12 packaging, standalone executable work, ecosystem documentation, or dependency upgrades.
- Deleting the Rust builtin implementation immediately, changing the frozen native API inventories, or redesigning the native TUI engine. Compatibility code is removed or deprecated in the tranche assigned by the architecture handoff, not opportunistically here.

## Commits

### Commit 1 — Define the TS tool contract and native lifecycle adapter

**Why:** The extension loader and native bindings need one stable contract before individual tools can migrate. The contract must make execution and presentation inseparable while keeping lifecycle ownership generic.

**Files:**

- `packages/iyon-sdk/src/tool.ts`
- `packages/iyon-sdk/src/index.ts`
- `packages/iyon-runtime/src/tools/contract.ts`
- `packages/iyon-runtime/src/tools/execution.ts`
- `packages/iyon-runtime/src/tools/generic.ts`
- `packages/iyon-runtime/src/index.ts`
- `packages/iyon-runtime/test/tools/execution.test.ts`
- `packages/iyon-runtime/test/tools/contract.test.ts`

**Work:**

- Add the public `Tool<TArgs, TResult>`/`defineTool` shape with `name`, `description`, `inputSchema`, execution metadata, `async execute(ctx, args)`, `renderCall(call)`, and `renderResult(result)`. Keep schemas serializable and compatible with the existing `ModelToolSpec`; use the existing SDK/native content and `View` types rather than introducing parallel JSON or TUI models.
- Model `ToolContext` with owned session/turn/call identifiers, cwd/workspace handles, `AbortSignal`, typed native update methods, and only the public capabilities already exported by `iyon:core`. Model canonical `ToolResult` with content, details, `isError`, and `terminate` so the transcript adapter does not infer success from rendered text.
- Add generic lifecycle dispatch that obtains a native `ToolExecution`, runs T6 before/after hooks, awaits the native approval transition, invokes the selected contribution, forwards progress/details updates, and calls native finish/fail exactly once. Convert thrown errors to the existing typed error result and preserve cancellation as cancellation.
- Add a generic fallback renderer that displays an unknown tool name, lifecycle status, JSON argument preview when enabled, result text, and error state using only TUI primitives. Keep collapse/clamp decisions in the generic transcript host, not in individual tool renderers.
- Register no bundled tool here. The adapter must select by the resolved contribution object, never by a tool-name conditional.

**Tests / verification:**

- `bun test packages/iyon-runtime/test/tools/contract.test.ts packages/iyon-runtime/test/tools/execution.test.ts`
- Assert lifecycle event order for success, before-hook block, approval rejection, thrown execution error, after-hook failure, and abort; assert native finish/fail is called once and the canonical result preserves `details` and `isError`.
- Assert generic rendering handles an unregistered `weather` tool and that no tool-name-specific branch exists in the runtime dispatcher.
- `bun run typecheck` (or the repository’s T1–T6 TypeScript typecheck command).

**Must not:**

- Add a Rust-owned executor, callback from a Tokio task into arbitrary JS, a second extension loader, or a privileged builtin path.
- Implement any of the seven tools, the bash policy, T9 agent policy, or T10 app code in this commit.

### Commit 2 — Port shared output, process, mutation, and diff semantics

**Why:** The individual packages need small, tested adapters for behavior that is currently spread across `iyon-core` output/process helpers and the Rust app’s structured-diff renderer. Centralizing only these mechanical primitives avoids seven subtly different truncation or path-result implementations without moving tool ownership out of the packages.

**Files:**

- `packages/iyon-plugins/src/tools/support/output.ts`
- `packages/iyon-plugins/src/tools/support/process.ts`
- `packages/iyon-plugins/src/tools/support/mutations.ts`
- `packages/iyon-plugins/src/tools/support/diff.ts`
- `packages/iyon-plugins/src/tools/support/render.ts`
- `packages/iyon-plugins/src/index.ts`
- `packages/iyon-plugins/test/tools/support/output.test.ts`
- `packages/iyon-plugins/test/tools/support/process.test.ts`
- `packages/iyon-plugins/test/tools/support/diff.test.ts`

**Work:**

- Port the model-output limits and truthful truncation reports from `crates/iyon-core/src/tools/output.rs`: head/tail selection, byte/line/character accounting, first-line and partial-line flags, grep line limits, and notices/details. Keep the limits as named constants and return data, not presentation `View`s.
- Wrap the existing public native process capability for command lookup/capture, timeout, stderr handling, and `AbortSignal` cancellation. The wrapper must support the `rg`/`grep` and `fd`/`find` fallback decisions needed by the search tools without hiding non-zero errors.
- Expose the existing native per-path mutation serialization as a typed helper for `write` and `edit`; do not replace it with an uncoordinated Bun `writeFile` sequence.
- Port unified-diff generation/normalization as a data helper and expose a small semantic renderer helper for parsing/rendering structured diffs through `iyon:tui`. Keep the full result available so the host can clamp only the displayed rows.
- Keep workspace resolution and permission checks in the earlier native façade. The support module may adapt its handles and error types but must not duplicate a weaker path sandbox.

**Tests / verification:**

- `bun test packages/iyon-plugins/test/tools/support`
- Port boundary cases from the Rust tests: exact byte/line limits, long grep lines, empty output, process exit code 1 for no matches, cancellation before spawn and during capture, CRLF/BOM normalization, new-file diff from empty, and malformed unified diff fallback.
- Compare representative TS helper results with the Rust compatibility tests or golden JSON fixtures; assert no output is silently truncated without a details notice.
- `bun run typecheck`.

**Must not:**

- Add a third-party process, diff, or schema dependency without approval.
- Put a tool name switch in shared support or move tool-specific call/result copy into this package.
- Change the native workspace permission contract or delete Rust helpers that the compatibility driver still uses.

### Commit 3 — Port read-only filesystem tools

**Why:** `read`, `ls`, `find`, and `grep` have no mutation approval flow and can establish the complete package shape, schema fidelity, fallback process behavior, and per-contribution rendering before the higher-risk mutation and shell tools move.

**Files:**

- `plugins/tools/read/package.json`
- `plugins/tools/read/src/index.ts`
- `plugins/tools/read/src/execute.ts`
- `plugins/tools/read/src/render.ts`
- `plugins/tools/read/test/read.test.ts`
- `plugins/tools/ls/package.json`
- `plugins/tools/ls/src/index.ts`
- `plugins/tools/ls/src/execute.ts`
- `plugins/tools/ls/src/render.ts`
- `plugins/tools/ls/test/ls.test.ts`
- `plugins/tools/find/package.json`
- `plugins/tools/find/src/index.ts`
- `plugins/tools/find/src/execute.ts`
- `plugins/tools/find/src/render.ts`
- `plugins/tools/find/test/find.test.ts`
- `plugins/tools/grep/package.json`
- `plugins/tools/grep/src/index.ts`
- `plugins/tools/grep/src/execute.ts`
- `plugins/tools/grep/src/render.ts`
- `plugins/tools/grep/test/grep.test.ts`

**Work:**

- Make each package’s `src/index.ts` export one `defineTool` contribution and package metadata only; package activation must discover it through the normal T6 entrypoint.
- Port `read` validation and UTF-8 workspace reading, including cancellation and `{path}` details. Port `ls` sorting, dotfiles, directory `/` suffixes, empty-directory text, limit handling, and model-byte truncation. Port `find`’s glob/path behavior, `fd` preference and `find` fallback, git/node_modules exclusions, relative POSIX paths, result limits, and byte truncation. Port `grep`’s regex/literal/case/context/glob arguments, `rg` preference and `grep` fallback, no-match handling, line and byte limits, and truncation details.
- Keep each package’s `renderCall` and `renderResult` beside its executor. Preserve the current compact call summaries and status labels, but consume generic `ToolCall`/`ToolResult` lifecycle values and TUI semantic helpers. Do not register these renderers in a Rust map.
- Keep the current schemas and descriptions as the source-compatible contract, including `additionalProperties: false`, defaults, required fields, and parallel execution metadata.

**Tests / verification:**

- `bun test plugins/tools/read/test plugins/tools/ls/test plugins/tools/find/test plugins/tools/grep/test`
- Assert schema snapshots and execution results against the Rust behavior for valid/invalid paths, hidden/denied paths, empty directories/no matches, fallback binaries, relative output, limit notices, and cancellation.
- Assert each contribution exposes `execute`, `renderCall`, and `renderResult`; render call/result fixtures for running, finished, and error states through `iyon:tui`.
- `bun run typecheck` and `bun run build --filter './plugins/tools/{read,ls,find,grep}'` (or the equivalent Bun workspace build command).

**Must not:**

- Implement writes, edits, shell commands, sudo approval, tool-round selection, or generic app routing here.
- Call the Rust builtin modules directly or use a package-specific loader.

### Commit 4 — Port write and edit with serialized mutation semantics

**Why:** File mutation is the highest-risk filesystem portion of the bundled set. It needs its own review boundary for locking, truthful diffs, line endings, BOMs, legacy input normalization, and error-safe lifecycle completion.

**Files:**

- `plugins/tools/write/package.json`
- `plugins/tools/write/src/index.ts`
- `plugins/tools/write/src/execute.ts`
- `plugins/tools/write/src/render.ts`
- `plugins/tools/write/test/write.test.ts`
- `plugins/tools/edit/package.json`
- `plugins/tools/edit/src/index.ts`
- `plugins/tools/edit/src/execute.ts`
- `plugins/tools/edit/src/render.ts`
- `plugins/tools/edit/test/edit.test.ts`

**Work:**

- Port `write` with exact path/content validation, native safe write resolution, parent-directory creation, per-path mutation queue, cancellation checks before/after I/O, existing-text handling, and the current success/error result text. Generate a diff only when the prior content is known; never fabricate a diff for a non-UTF-8 prior file.
- Port `edit`’s `edits[]` schema and legacy `oldText`/`newText` normalization, non-empty/unique matching, matching against the original content, range sorting and overlap rejection, line-ending restoration, BOM preservation, first-changed-line details, serialized writes, and truthful unified diff output.
- Implement `write` and `edit` renderers from the existing Rust behavior: successful summary before structured diff, error results that do not dump written payloads, malformed-diff raw fallback, and full result retention while the host applies the collapsed-row clamp.
- Mark both contributions sequential and keep approval metadata generic (`NeverAsk` unless product policy from an earlier tranche overrides it); do not encode approval flow in the executor.

**Tests / verification:**

- `bun test plugins/tools/write/test plugins/tools/edit/test`
- Port the existing Rust mutation tests and renderer assertions: new-file diff, replacement diff, non-UTF-8 prior file, CRLF and BOM preservation, multiple disjoint edits, duplicate/overlapping/empty edits, legacy payloads, cancellation, failed write, summary/diff ordering, hidden error payloads, and malformed diff visibility.
- Run the mutation tests with concurrent calls to the same path and assert serialization; run different paths concurrently and assert they are not globally serialized.
- `bun run typecheck` and the filtered workspace build.

**Must not:**

- Introduce a second file-lock implementation, bypass native workspace permissions, or change the native mutation queue contract.
- Fold edit/write policy into the app or make the renderer responsible for file I/O.

### Commit 5 — Port bash execution and move sudo policy into contributions

**Why:** Bash combines process lifetime, streaming updates, timeout/cancellation, rolling output, full-output spill files, and approval policy. Keeping it separate makes the native lifecycle and security review explicit.

**Files:**

- `plugins/tools/bash/package.json`
- `plugins/tools/bash/src/index.ts`
- `plugins/tools/bash/src/execute.ts`
- `plugins/tools/bash/src/render.ts`
- `plugins/tools/bash/src/policy.ts`
- `plugins/tools/bash/test/bash.test.ts`
- `plugins/tools/bash/test/policy.test.ts`
- `packages/iyon-runtime/src/tools/policy.ts`
- `packages/iyon-runtime/test/tools/policy.test.ts`

**Work:**

- Port the bash schema, shell resolution, command validation, cwd, optional timeout, stdout/stderr collection, rolling two-model-limit buffer, periodic `ToolUpdate.Text`/details updates, cancellation, exit code handling, and temporary full-output file behavior from the Rust builtin. Preserve `fullOutputPath` details and the exact truncated-result notice.
- Make the command run through the existing public native process capability or Bun’s normal process API as exposed by the tool context; the Rust kernel must not own or resume the process. Tie every long-lived process to the supplied `AbortSignal` and ensure child cleanup on timeout/cancel.
- Move the existing `bash_command_uses_sudo` decision into a typed product/tool policy contribution. The policy may inspect command text because it belongs to bash policy, but generic runtime dispatch must not inspect `toolName === "bash"`.
- Port bash call/result presentation, including command preview, status label, error styling, and full-output path warning, into `render.ts`.

**Tests / verification:**

- `bun test plugins/tools/bash/test packages/iyon-runtime/test/tools/policy.test.ts`
- Assert schema/defaults, shell selection, merged/separate output, rolling updates, timeout, abort kills the child, non-zero exit, spill-file details, and no fabricated success after process failure.
- Assert sudo commands pass through the policy contribution and non-sudo commands do not request approval; assert a third-party policy can replace the policy layer without changing lifecycle dispatch.
- `bun run typecheck` and the filtered workspace build.

**Must not:**

- Add a generic sudo branch, make all commands require approval, shell out through a Rust-owned process manager, or expose privileged APIs to the default agent.
- Change T9’s tool-round policy or T10’s approval UI.

### Commit 6 — Register all bundled tools through the ordinary extension loader

**Why:** The tool packages are only real extensions once the bundled product loads them through the same activation transaction and replacement rules as third-party packages.

**Files:**

- `packages/iyon-plugins/src/bundled-tools.ts`
- `packages/iyon-plugins/src/index.ts`
- `packages/iyon-plugins/test/bundled-tools.test.ts`
- `packages/iyon-plugins/test/fixtures/replace-read/package.json`
- `packages/iyon-plugins/test/fixtures/replace-read/src/index.ts`
- `packages/iyon-plugins/test/fixtures/replace-read/src/render.ts`
- `packages/iyon-plugins/test/fixtures/replace-read/src/execute.ts`
- `plugins/tools/{bash,read,write,edit,grep,find,ls}/package.json`

**Work:**

- Define the bundled package list as ordinary package metadata/entrypoints and feed it to the T6 activation transaction. Do not construct a parallel map of privileged builtin executors or renderers. Activation must register all seven contributions, their schemas, and their policy layers before the agent can request tool specs.
- Export the resolved registry/activation result for T9 and T10 without exposing mutable internal maps. A caller should be able to enumerate definitions, resolve a tool by ID, and layer an explicit replacement.
- Add a fixture package that replaces `read` with a contribution whose execution returns a marker result and whose call/result renderers return visibly distinct marker views. Activate it with `replace: true` and assert the old execution and both old renderers are unreachable. Assert failed activation rolls the complete replacement back and leaves the original `read` intact.
- Add an unknown-tool fixture/call that uses the generic renderer and does not alter the bundled registry.

**Tests / verification:**

- `bun test packages/iyon-plugins/test/bundled-tools.test.ts`
- Assert all seven IDs are loaded by the same loader path used for a synthetic third-party package, duplicate registration fails, explicit replacement is atomic, and package activation rollback leaves no partial schema/execution/render registration.
- Assert the registry contains no `isBuiltin` flag, no separate builtin loader, and no per-tool render selection outside contribution objects.
- `bun run typecheck` and a clean workspace build.

**Must not:**

- Let the fixture replace only execution or only presentation; the override test must prove the complete contribution is replaced atomically.
- Add an allowlist that prevents third-party tools from using the same lifecycle or filesystem/process APIs.

### Commit 7 — Remove Rust per-tool presentation from the compatibility app

**Why:** T10 will later port the default app, but it must not inherit the current Rust renderer table. T8 therefore moves the presentation boundary now and leaves the compatibility driver generic-only.

**Files:**

- `crates/iyon/src/tui/tools/registry.rs`
- `crates/iyon/src/tui/tools/mod.rs`
- `crates/iyon/src/tui/tools/renderers/mod.rs`
- `crates/iyon/src/tui/tools/renderers/generic.rs`
- `crates/iyon/src/tui/tools/renderers/{bash,read,write,edit,grep,find,ls}.rs` (delete)
- `crates/iyon/src/tui/transcript/semantic.rs`
- `crates/iyon/src/tui/components/conversation_activity.rs`
- `crates/iyon/src/tui/backend.rs`
- `crates/iyon/src/tui/state.rs`

**Work:**

- Remove the hard-coded Rust renderer registrations and per-tool renderer modules. Keep the generic renderer and generic timeline state; pass contribution-rendered semantic views into the compatibility presentation boundary or use the generic renderer when the compatibility driver has no TS contribution available.
- Keep lifecycle/status/collapse behavior generic. `TuiFormatter` may still format a tool call/result, but it must not know the names or schemas of bundled tools. Preserve unknown-tool rendering.
- Update compatibility event mapping so native lifecycle events and canonical transcript results remain intact while tool-specific presentation is supplied by the Bun extension path. Do not move approval, transcript ownership, or terminal layout into Rust to compensate.
- Audit `crates/iyon-core/src/runtime.rs` and the compatibility construction path after T3: if an old default-registration call remains, replace only the product wiring with explicit registry injection/empty compatibility setup. Leave `crates/iyon-core/src/tools/builtin/*` and its direct compatibility tests buildable; they must not be reachable from the Bun product path.

**Tests / verification:**

- `cargo test -p iyon --lib tui::transcript tui::tools`
- `cargo test --workspace`
- Assert generic rendering for an unknown tool, status transitions, argument preview clamping, result clamping, approval/cancelled states, and native event-to-transcript mapping.
- Run `rg -n 'tool_name.*(bash|read|write|edit|grep|find|ls)|name.*==.*(bash|read|write|edit|grep|find|ls)|BashRenderer|ReadRenderer|EditRenderer|GrepRenderer|FindRenderer|LsRenderer|WriteRenderer' crates/iyon/src` and require zero product-path presentation matches.
- Run `rg -n 'register_builtin_defaults|tools/builtin' packages crates/iyon-native crates/iyon-cli 2>/dev/null` and require any remaining matches to be compatibility-only declarations/tests, not Bun runtime imports or product construction.

**Must not:**

- Delete the compatibility executors prematurely, reimplement TUI layout in TypeScript, or change the Rust public API inventory.
- Add a Rust-side name-based fallback that T10 would have to remove later.

### Commit 8 — Prove the Bun product path and tranche exit invariants

**Why:** The migration is complete only when execution, rendering, registration, replacement, native lifecycle, and compatibility boundaries are verified together rather than by isolated package tests.

**Files:**

- `packages/iyon-runtime/test/tools/bundled-lifecycle.integration.test.ts`
- `packages/iyon-plugins/test/tools/bundled-path.integration.test.ts`
- `tools/api-surface/` (only the T2-generated disposition/verification artifact if a new public tool-lifecycle path is exposed)
- Existing root `package.json` test/typecheck script entries, if needed to include the new workspace tests
- Existing CI workflow file that already runs Rust and Bun checks, if needed to include the new commands; do not create a second CI workflow

**Work:**

- Drive a synthetic model tool call through the Bun-owned registry for each bundled tool and assert the complete lifecycle event sequence, canonical transcript result, native state transitions, and contribution-specific semantic render.
- Run the `replace-read` fixture through the same product registry and assert execution plus both render methods change atomically without modifying the runtime/app code.
- Exercise an unknown third-party tool and assert generic call/result rendering, normal lifecycle completion, and no bundled-tool special case.
- Add static exit audits for no main-path dependency on `iyon-core/src/tools/builtin/*`, no per-tool-name presentation in the Rust app or generic TypeScript runtime, and no separate bundled loader.
- Update only existing root scripts/CI invocations so the tranche checks run in review: Rust workspace tests/checks, Bun tests, TypeScript typecheck, and the focused override/lifecycle integration tests.

**Tests / verification:**

- `cargo fmt --all -- --check`
- `cargo check --workspace`
- `cargo test --workspace`
- `bun test packages/iyon-runtime/test packages/iyon-plugins/test plugins/tools`
- `bun run typecheck`
- `test -z "$(rg -n 'if .*tool(Name|_name).*===?.*(bash|read|write|edit|grep|find|ls)|match .*tool_name' crates/iyon/src packages/iyon-runtime/src 2>/dev/null)"` after narrowing the audit to product/runtime code and allowing contribution-local policy/render files.
- Assert the final registry resolves seven bundled IDs, third-party replacement is atomic, unknown tools are generic, native finish/fail occurs exactly once, and all Rust builtin matches are compatibility-only.

**Must not:**

- Expand this commit into T9 agent policy, T10 app/CLI migration, T11 performance work, or T12 packaging.
- Mark the tranche complete while any product construction still imports `iyon-core/src/tools/builtin/*`, any Rust app registry still names a bundled renderer, or the override fixture replaces less than execution plus both renderers.

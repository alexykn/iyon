# Iyon migration matrix

This document maps the current Rust workspace to the Bun + TypeScript +
N-API target. It is a migration contract, not an implementation checklist.
Each row names its owning tranche and its compatibility disposition. No row is
complete until that tranche's exit criteria pass.

## Current-to-target matrix

| current location | target location | migration tranche | compatibility disposition |
|---|---|---|---|
| `crates/iyon-api/src/client.rs`, `error.rs`, `model.rs`, `stream.rs` | `crates/iyon-api` protocol library exposed through `iyon:api` | T4 | Keep the Rust protocol paths and semantics while the binding lands; public paths remain compatibility surfaces. |
| `crates/iyon-api/src/providers/mock.rs` | `plugins/providers/mock` | T7 | Product stops using the Rust implementation when the TS extension is active; retain deprecated Rust provider type temporarily. |
| `crates/iyon-api/src/providers/openrouter.rs` | `plugins/providers/openrouter` | T7 | Product stops using the Rust implementation when the TS extension is active; retain deprecated Rust provider type temporarily. |
| `crates/iyon-api/src/providers/openai_codex.rs` | `plugins/providers/openai-codex` | T7 | Product stops using the Rust implementation when the TS extension is active; retain deprecated Rust provider type temporarily. |
| `crates/iyon-api/src/providers/mod.rs` and root provider re-exports | `iyon:api` compatibility surface plus bundled provider registrations | T4, T7 | Preserve Rust re-exports during binding; remove concrete provider types only in the breaking release following T7. |
| `crates/iyon-core/src/ids.rs` | `crates/iyon-core` provider-agnostic kernel | T3 | Preserve ID meanings and public compatibility paths while the kernel is extracted. |
| `crates/iyon-core/src/session/**`, `src/agent/state.rs`, `src/agent/transcript.rs` | `crates/iyon-core` kernel session, transcript, and invariant state | T3 | Keep native state and correctness boundaries; `IyonCore` remains a compatibility façade. |
| `crates/iyon-core/src/command.rs`, `src/event.rs` | `crates/iyon-core` kernel commands/events exposed through `iyon:core` | T3, T4 | Preserve event ordering, variants, and public paths through the compatibility layer. |
| `crates/iyon-core/src/agent/control.rs`, `src/agent/loop.rs`, `src/agent/request.rs`, `src/agent/turn.rs` | `crates/iyon-core` lifecycle primitives; agent policy in `plugins/agents/iyon` | T3, T9 | Split native session/lifecycle state from default agent behavior; do not retain the default agent as the kernel owner. |
| `crates/iyon-core/src/agent/tool_call.rs`, `src/agent/tool_execution.rs` | `crates/iyon-core` tool execution contracts and cancellation; product agent orchestration in `plugins/agents/iyon` | T3, T8, T9 | Native execution and cancellation remain stable; policy and registration move to TS extensions. |
| `crates/iyon-core/src/fs/**` | `crates/iyon-core` filesystem/process primitives exposed through `iyon:core` | T3, T4 | Preserve native boundary checks and behavior; expose capability paths through the public binding. |
| `crates/iyon-core/src/runtime.rs` | `crates/iyon-core` kernel runtime/lifecycle | T3 | Keep native ownership of runtime state; `IyonCore` compatibility remains until its planned removal. |
| `crates/iyon-core/src/tools/definition.rs`, `executor.rs`, `hooks.rs`, `output.rs`, `process.rs`, `registry.rs`, `file_mutation_queue.rs` | `crates/iyon-core` kernel tool contracts, hooks, output policy, and process primitives | T3 | Retain native correctness contracts; do not collapse kernel state into TypeScript. |
| `crates/iyon-core/src/tools/builtin/bash.rs` | `plugins/tools/bash` | T8 | Move the builtin implementation and presentation integration; no main path may depend on a native builtin registration. |
| `crates/iyon-core/src/tools/builtin/read.rs` | `plugins/tools/read` | T8 | Move the builtin implementation and presentation integration; preserve execution and output behavior. |
| `crates/iyon-core/src/tools/builtin/write.rs` | `plugins/tools/write` | T8 | Move the builtin implementation and presentation integration; preserve file mutation behavior. |
| `crates/iyon-core/src/tools/builtin/edit.rs` | `plugins/tools/edit` | T8 | Move the builtin implementation and presentation integration; preserve edit and diff behavior. |
| `crates/iyon-core/src/tools/builtin/grep.rs` | `plugins/tools/grep` | T8 | Move the builtin implementation and presentation integration; preserve search/output behavior. |
| `crates/iyon-core/src/tools/builtin/find.rs` | `plugins/tools/find` | T8 | Move the builtin implementation and presentation integration; preserve glob/search behavior. |
| `crates/iyon-core/src/tools/builtin/ls.rs` | `plugins/tools/ls` | T8 | Move the builtin implementation and presentation integration; preserve directory listing behavior. |
| `crates/iyon-core/src/tools/builtin/mod.rs` and builtin registration helpers | `plugins/tools/*` registrations through `iyon:plugins` | T8 | Remove hidden native default registration from the product path; retain compatibility only until the tranche exit gate passes. |
| `crates/iyon-tui/src/application/**` | `crates/iyon-tui` native terminal runtime, exposed through `iyon:tui` | T5 | Keep native host, lifecycle, sizing, input, and async handle behavior; retain public Rust API compatibility. |
| `crates/iyon-tui/src/terminal/**`, `src/backend/**` | `crates/iyon-tui` native terminal backends behind `iyon-native` | T5 | Remain native; TypeScript receives owned runtime/handle operations rather than terminal logic. |
| `crates/iyon-tui/src/history/**`, `src/history/native/**` | `crates/iyon-tui` native `History` and scrollback behind `iyon:tui` handles | T5 | Preserve physical promotion, native sink, pressure drain, and model synchronization behavior. |
| `crates/iyon-tui/src/component/**`, `src/controls/**` | `crates/iyon-tui` native components and controls with `iyon:tui` handles | T5 | Stateful components and `TextInput` remain native handles; preserve public component/control behavior. |
| `crates/iyon-tui/src/geometry/**` | `crates/iyon-tui` native geometry primitives | T5 | Keep geometry native and derived from resolved semantic views; no TS geometry policy fork. |
| `crates/iyon-tui/src/presentation/api/**`, `src/presentation/ir.rs` | `crates/iyon-tui` semantic `View` values and lazy TS façades through `iyon:tui` | T5 | Preserve semantic builder behavior; a façade may batch/lazily lower values rather than issue one native call per builder method. |
| `crates/iyon-tui/src/presentation/layout/**` | `crates/iyon-tui` native layout engine | T5 | Remain native; resolve semantic identity before geometry and do not make application semantics depend on measured geometry. |
| `crates/iyon-tui/src/presentation/paint/**` | `crates/iyon-tui` native paint engine | T5 | Remain native; theme selection happens at semantic paint time. |
| `crates/iyon-tui/src/content/text/**`, `src/content/render.rs` | `crates/iyon-tui` semantic text and renderer paths through `iyon:tui` | T5 | Preserve semantic text identity, source projection, and renderer compatibility. |
| `crates/iyon-tui/src/content/diff/**` | `crates/iyon-tui` native semantic diff/value path through `iyon:tui` | T5 | Preserve public diff types and rendering behavior; no product-specific diff fork. |
| `crates/iyon-tui/src/projection/**` | `crates/iyon-tui` native projection and `Smooth` handles/values | T5 | Preserve stability, atomic spans, value-count pacing, and seal behavior. |
| `crates/iyon-tui/src/stream/**` | `crates/iyon-tui` native stream mechanics and owned stream handles | T5 | Preserve stream ordering, stable content slots, viewport anchoring, and cancellation ownership. |
| `crates/iyon-tui/src/scene/**` | `crates/iyon-tui` native Scene host plus `iyon:plugins` Scene extension integration | T5, T6 | Keep Scene distinct from app selection and ordinary View composition; preserve native resolve/layout/paint ownership. |
| `crates/iyon-tui/src/interaction/**`, `src/output/**` | `crates/iyon-tui` native routing/output paths exposed through `iyon:tui` | T5 | Preserve key, paste, focus, routing, output, and cancellation semantics. |
| `crates/iyon-tui/src/theme/**` | `crates/iyon-tui` native theme definitions and semantic paint selectors | T5 | Preserve named styles and variants; theme must not become geometry, wrapping, or sizing policy. |
| `crates/iyon-tui/src/testing.rs`, `tests/**`, and test harness exports | `crates/iyon-tui` native harness plus TS binding harness | T5 | Keep Rust behavior and public harness compatibility; add binding tests only in T5 implementation. |
| `crates/iyon-tui/src/lib.rs` public re-exports and prelude | `iyon:tui` public TypeScript surface backed by `crates/iyon-tui` | T5 | Treat every reachable path, alias, field, variant, associated item, feature/cfg path, and re-export as parity scope. |
| `crates/iyon/src/main.rs` | `packages/iyon-cli` Bun CLI and standalone executable | T10 | Remove the Rust binary entry only in T10; keep the Rust compatibility library until its removal tranche. |
| `crates/iyon/src/lib.rs` | `crates/iyon` deprecated compatibility library | T10, T12 | Preserve as a compatibility library; do not use it as the final executable owner. |
| `crates/iyon/src/auth.rs` | provider/app extensions under `plugins/providers/*` and `plugins/app/iyon` | T7, T10 | Preserve startup/auth/provider selection semantics while ownership moves to TS packages. |
| `crates/iyon/src/tui/backend.rs` | `plugins/app/iyon` app/backend integration over `iyon:core` and `iyon:tui` | T10 | Preserve event mapping, `ToolDraftKey`, approval identity, and cancellation behavior in the bundled app. |
| `crates/iyon/src/tui/controller.rs`, `src/tui/state.rs` | `plugins/app/iyon` application state, composer, transcript orchestration, and shutdown policy | T10 | Preserve UX and action behavior; native TUI primitives remain owned by `iyon-tui`. |
| `crates/iyon/src/tui/components/**` | `plugins/app/iyon` application-specific conversation and steering components | T10 | Move product widgets as TS app code; do not turn them into generic native TUI ownership. |
| `crates/iyon/src/tui/theme.rs` | `plugins/app/iyon` bundled theme contribution | T10 | Preserve palette and semantic selectors; native theme/paint APIs remain in `iyon-tui`. |
| `crates/iyon/src/tui/transcript/**` | `plugins/app/iyon` assistant stream, transcript, and pacing policy | T10 | Preserve stream boundaries, smoothing, tool-card ordering, and assistant presentation behavior. |
| `crates/iyon/src/tui/tools/renderers/bash.rs` | `plugins/tools/bash` | T8 | Move renderer beside its tool contribution; the app must not dispatch by tool name as its ownership boundary. |
| `crates/iyon/src/tui/tools/renderers/read.rs` | `plugins/tools/read` | T8 | Move renderer beside its tool contribution; preserve output presentation. |
| `crates/iyon/src/tui/tools/renderers/write.rs` | `plugins/tools/write` | T8 | Move renderer beside its tool contribution; preserve output presentation. |
| `crates/iyon/src/tui/tools/renderers/edit.rs`, `unified_diff.rs` | `plugins/tools/edit` | T8 | Move renderer and diff presentation beside the edit tool; preserve diff behavior. |
| `crates/iyon/src/tui/tools/renderers/grep.rs` | `plugins/tools/grep` | T8 | Move renderer beside its tool contribution; preserve search presentation. |
| `crates/iyon/src/tui/tools/renderers/find.rs` | `plugins/tools/find` | T8 | Move renderer beside its tool contribution; preserve search presentation. |
| `crates/iyon/src/tui/tools/renderers/ls.rs` | `plugins/tools/ls` | T8 | Move renderer beside its tool contribution; preserve listing presentation. |
| `crates/iyon/src/tui/tools/renderers/generic.rs`, `mod.rs`, `registry.rs`, `types.rs` | `plugins/tools/*` contribution renderers and shared app integration | T8, T10 | Replace the app-side tool-name switch with contribution metadata and preserve the generic fallback behavior. |
| `crates/iyon/tests/public_app.rs` and existing application tests | `plugins/app/iyon` parity test coverage | T10 | Use as the behavioral oracle; do not alter assertions in T0. |
| `iyon-api.md` | repository-root read-only inventory input to `tools/api-surface/` | T2 | Keep human-readable and unchanged; never classify as generated output. |
| `iyon-core.md` | repository-root read-only inventory input to `tools/api-surface/` | T2 | Keep human-readable and unchanged; never classify as generated output. |
| `iyon-tui.md` | repository-root read-only inventory input to `tools/api-surface/` | T2 | Keep human-readable and unchanged; never classify as generated output. |
| `iyon.md` | repository-root read-only inventory input to `tools/api-surface/` and app/CLI migration baseline | T2, T10 | Keep human-readable and unchanged; preserve documented startup/provider/auth semantics. |

## Migration splits and ownership notes

The important splits are intentional:

- `iyon-api` keeps the Rust model protocol, while concrete provider
  implementations move to `plugins/providers/*` in T7. Deprecated Rust provider
  types remain until the planned breaking removal; product use stops first.
- `iyon-core` keeps native IDs, state, lifecycle, cancellation, filesystem and
  process primitives, tool contracts, hooks, output policy, and transcript
  invariants in T3. The default agent's behavior moves to
  `plugins/agents/iyon` in T9. The native `IyonCore` handle remains a
  compatibility façade during extraction.
- `iyon-tui` keeps terminal correctness, layout, paint, history, streams,
  projections, controls, and native handles. The application supplies semantic
  values and policy through `iyon:tui`; it does not reimplement terminal or
  layout logic.
- The application crate is split by responsibility: `main.rs` becomes the
  Bun CLI in T10; auth and provider selection move through T7/T10; app
  components, controller/state, theme, and transcript move to
  `plugins/app/iyon`; tool renderers move with their tool packages in T8.
- The selected app, Scene extensions, and app registration remain distinct.
  A replacement app is selected as the base app; it does not first construct
  the bundled app. Scene extensions then compose or replace the resulting
  generic Scene.

## Compatibility rules

1. Preserve public Rust paths and behavior while `iyon:*` bindings land.
2. Keep `crates/iyon` as a deprecated compatibility library until the planned
   deprecation/removal tranche; do not delete it as part of an intermediate
   migration.
3. Do not create hidden native default registrations. Bundled and external
   extensions use the same loader and registry path.
4. A provider, tool, agent, or app row is a contribution integration point, not
   a permission boundary or isolated extension runtime.
5. Do not claim a target location is complete until its numbered owning tranche
   exit criteria pass.
6. T0 records this matrix and freezes behavior; it moves no real source file.

## Extension, binding, and Scene ownership notes

The extension runtime must discover a package, parse its manifest, validate
compatibility, resolve its TypeScript entrypoint, activate it transactionally,
validate registrations, commit the contributions, and roll back all
contributions on import or activation failure. Duplicate IDs fail by default;
an explicit `replace: true` creates a layered replacement carrying source
metadata. Unload disposes layers in reverse order and restores the previous
layer. Bundled and external packages use this same loader; there is no
`is_builtin` path.

`iyon:plugins` owns the registries for tools, providers, agents, apps,
commands, shortcuts, and Scene extensions. A registration supplies integration
metadata, not an execution restriction. An extension may register more than
one contribution type and may use ordinary Bun APIs or lower-level public
surfaces.

The app registry selects one base app. That app produces a generic Scene, then
ordered Scene extensions compose or replace it before the terminal host runs
the final Scene. Scene extension registration does not select an app, and an
app replacement does not construct the bundled app first. Ordinary `View`
composition remains separate from both app selection and Scene extension
composition.

The binding contract is also split by ownership: `iyon:api` exposes the Rust
model protocol; `iyon:core` exposes provider-agnostic kernel capabilities;
`iyon:tui` exposes semantic values plus native handles for stateful TUI
objects; and `iyon:plugins` exposes lifecycle and registries. The bridge is
not a product layer, and binding completeness is semantic parity rather than
one native symbol per Rust method.

# Iyon architecture

> Bun owns control flow, product behavior, and extensibility. Rust owns
> correctness-critical native state and the high-performance terminal engine.

This document is the target architecture contract. The current Rust workspace
is a migration baseline; it is not evidence that the final executable or the
application ownership model should remain Rust-owned.

## Governing ownership

Bun owns the process, startup, control flow, product policy, extension loading,
application composition, and the final executable. The bundled Iyon app,
agent, tools, and providers use the same extension mechanisms available to
third-party packages.

Rust remains the native implementation for correctness-critical state and
terminal performance. It owns the model protocol types, the provider-agnostic
execution kernel, terminal I/O, layout, wrapping, paint, native `History`,
stream and projection mechanics, `TextInput` state, terminal sizing, and input.
Rust does not own product policy or extension behavior.

There is no production Rust application entrypoint that owns the process. The
canonical executable is the Bun-compiled TypeScript CLI, plus the native addon
embedded or distributed beside it according to the platform packaging policy.

## Final repository shape

The target repository has this shape:

```text
.
├── Cargo.toml
├── Cargo.lock
├── package.json
├── bun.lock
├── tsconfig.json
├── ARCHITECTURE.md
├── MIGRATION.md
├── crates/
│   ├── iyon-api/
│   ├── iyon-core/
│   ├── iyon-tui/
│   ├── iyon-native/
│   └── iyon/                  # temporary Rust compatibility library
├── packages/
│   ├── iyon-runtime/
│   ├── iyon-sdk/
│   ├── iyon-plugins/
│   └── iyon-cli/
├── plugins/
│   ├── app/iyon/
│   ├── agents/iyon/
│   ├── providers/{mock,openrouter,openai-codex}/
│   └── tools/{bash,read,write,edit,grep,find,ls}/
└── tools/
    └── api-surface/
```

`packages/*` and the bundled `plugins/*` directories are ordinary Bun
workspace packages. `tools/api-surface/` owns the later API reachability and
parity tooling. `crates/iyon` is retained temporarily as a deprecated Rust
compatibility library; it is not the final executable owner and is removed
only by its planned compatibility tranche.

## Dependency graphs

The Rust graph is deliberately narrow and frozen:

```text
iyon-api   → no Iyon dependencies
    ▲
    │
iyon-core  → iyon-api

iyon-tui   → no iyon-core, no iyon-api

iyon-native → iyon-api, iyon-core, iyon-tui
```

`iyon-native` is the N-API bridge. It contains only binding, marshalling,
resource ownership, error conversion, and lifecycle code. It is not a product
or application layer. Provider selection, agent behavior, tool registration,
extension policy, and app composition must not accumulate there.

The TypeScript graph is:

```text
iyon-cli
   │
   ▼
iyon-runtime ───── iyon-plugins
   │                    │
   │                    ▼
   │              loaded extensions
   │
   ├── iyon:api
   ├── iyon:core
   ├── iyon:tui
   └── iyon:plugins
```

`iyon-runtime` loads `iyon-plugins` and exposes the canonical virtual modules
`iyon:api`, `iyon:core`, `iyon:tui`, and `iyon:plugins`. `@iyon/sdk` supplies
development-time declarations and authoring types. Extension authors are not
required to install or call the native addon directly.

## Bun runtime path and native membrane

The normal execution path is:

```text
TypeScript CLI or extension
        ↓
iyon-runtime virtual module
        ↓
iyon-native.node operation
        ↓
owned Promise / typed result
        ↓
TypeScript continuation
```

The membrane is organized around semantic operations, not a generated native
symbol for every Rust method. Immutable semantic values such as `View` may be
lazy TypeScript façades. Stateful or performance-sensitive objects such as
`History`, `TextInput`, streams, components, and the terminal runtime use
native handles. The bridge must not serialize native TUI state merely to make
the boundary convenient, and TypeScript must not duplicate terminal or layout
logic.

Every long-lived native operation has an explicit owner and cancellation path.
The TypeScript façade bridges `AbortSignal` and explicit cancellation handles
or tokens to native cancellation. Dropping a JavaScript Promise is not itself
a cancellation protocol. Native operations return owned Promises and typed
errors; a Rust panic must not unwind through Bun.

Rust must own all data that crosses a suspended asynchronous boundary. A
borrowed N-API value must not remain live while a Rust future is suspended.
Background tasks must not outlive the native handle or runtime state they use.
Data/value crossings are preferred to callbacks; when Rust genuinely calls
JavaScript, it must marshal onto the JavaScript thread using the supported
native mechanism.

## Packages, extensions, and contributions

These terms are distinct:

| Term | Meaning |
|---|---|
| **Package** | A distribution unit containing metadata, dependencies, and an entrypoint. |
| **Extension** | An executable TypeScript module loaded and activated by the runtime. |
| **Contribution** | A well-known registration made by an extension, such as an app, agent, provider, tool, command, shortcut, or Scene extension. |

An extension is arbitrary TypeScript running in normal Bun. It may use ordinary
Bun filesystem, process, network, and npm APIs, plus the public Iyon
APIs and lower-level hooks. The contribution type is integration metadata for
product surfaces; it is not a permission boundary, execution restriction, or
separate privileged runtime. Apps, agents, providers, and tools are not
different classes of executable extension.

The bundled extensions are ordinary packages loaded through the same runtime
path as external packages. The architecture has one extension mechanism and
does not introduce a second built-in path.

## App selection and Scene composition

App selection and Scene composition are separate concepts.

1. The runtime selects one base app.
2. The selected app produces a generic `Scene`.
3. Ordered Scene extensions compose or replace that Scene.
4. The final Scene is handed to the terminal UI host.

The generic shape is:

```ts
type Scene = {
  history?: History
  body: View
}
```

`Scene` is not the app registry and is not ordinary `View` composition. A
Scene extension can compose or replace the selected app's Scene. A
replacement app is selected as the base app; the runtime does not instantiate
the bundled app first and then erase it. This is the extension escape hatch
for alternative shells and complete application experiences.

## Native library ownership

| Area | Native owner | Contract |
|---|---|---|
| Model protocol | `iyon-api` | Rust model requests, messages, stream events, errors, and protocol compatibility. |
| Execution kernel | `iyon-core` | Provider-agnostic IDs, session/turn state, commands/events, cancellation, tool execution contracts, filesystem/process primitives, hooks, output policy, and transcript/session invariants. |
| Terminal engine | `iyon-tui` | Terminal I/O, sizing/input, semantic resolution, geometry/layout, wrapping, paint, native history/scrollback, streams, projections, smoothing, components, controls, theme, and testing harness. It has no dependency on `iyon-core` or `iyon-api`. |
| Native membrane | `iyon-native` | N-API binding, marshalling, native handles, owned async operations, cancellation, and typed error conversion only. |

`iyon-core` is a provider-agnostic native execution kernel, not the Iyon
agent. The default Iyon agent, bundled providers, and bundled tools become
TypeScript extensions. `iyon-api` remains the Rust model protocol during
migration. Concrete providers leave the long-term API crate in the provider
migration tranche, while deprecated Rust compatibility types remain until the
planned breaking removal.

`iyon-tui` remains the terminal-performance center. Semantic values may cross
through lazy TypeScript façades; stateful objects use native handles; async
work uses owned, cancellable Promise boundaries. A fluent `View` builder does
not require one N-API call per method.

## Registrations and integration

`iyon:plugins` provides registries for tools, providers, agents, apps,
commands, shortcuts, and Scene extensions. Registrations carry IDs, metadata,
source information, and the integration data needed by the selected app.

The meanings differ by contribution type:

| Registration | Product integration |
|---|---|
| App | Candidate base application selected at startup. |
| Agent | Agent behavior and turn orchestration available to an app. |
| Provider | Model/auth metadata and a model implementation available for selection. |
| Tool | Tool definition, execution, approval metadata, updates, and result behavior. |
| Scene extension | Ordered composition or replacement of the selected app's generic Scene. |
| Command / shortcut | User-facing actions routed into an app. |

These registrations do not make the extension execution model narrower. A
single extension may make multiple contribution registrations and may also
use lower-level APIs absent a contribution registration.

## Extension lifecycle

`iyon-runtime` uses one transactional lifecycle for bundled and external
packages:

```text
discover package
    → parse manifest
    → validate compatibility
    → resolve TypeScript entrypoint
    → activate transactionally
    → validate registrations
    → commit contributions
```

Import or activation failure rolls back every contribution made during that
activation. Duplicate IDs fail by default. A replacement must explicitly set
`replace: true`; replacement layers are ordered and retain source metadata.
Unloading disposes contributions in reverse order and restores the previous
layer when a replacement is removed. Disposal and rollback must release all
handles and owned async work from the activation.

There is no separate bundled-package activation path and no `is_builtin`
shortcut. The same discovery, validation, activation, registry, and disposal
rules apply to the bundled Iyon packages and third-party packages.

## Binding contract by virtual module

The canonical modules expose semantic contracts:

| Module | Public responsibility | Native boundary rule |
|---|---|---|
| `iyon:api` | Model protocol values, stream events, errors, and compatibility paths. | Keep protocol semantics in `iyon-api`; use owned values across async work. |
| `iyon:core` | Kernel IDs, session state, commands/events, cancellation, tools, hooks, and native execution primitives. | Use native handles for stateful kernel objects and explicit cancellation for long-lived operations. |
| `iyon:tui` | Semantic View values, native handles, streams, History, TextInput, Scene host, and terminal runtime. | Keep terminal/layout/paint/history native; use lazy value façades where a handle is unnecessary. |
| `iyon:plugins` | Registries, contribution metadata, lifecycle, and Scene extension composition. | Registry changes are transactional and carry source/layer ownership. |

Bindings are semantic boundaries. A Rust method is complete when a documented,
tested TypeScript path exists; it need not become a distinct N-API export.
`View` and related immutable builders may remain lazy TypeScript values. A
`History`, `TextInput`, stream, component, or terminal runtime remains a
native handle when it owns mutable state or performance-critical resources.
Long-lived operations return owned Promises, have explicit cancellation, and
surface typed errors. Borrowed native values never cross a suspended Rust
future.

## API parity and compatibility

The four root inventory documents are read-only migration baselines. Their
reachable public paths, aliases, fields, variants, associated items,
feature/cfg paths, and re-exports form the initial parity universe. They are
human-readable source inventories, not generated output and not disposable
documentation.

| Baseline file | Role in parity work |
|---|---|
| `iyon-api.md` | Read-only model protocol inventory consumed by `tools/api-surface/` in T2. |
| `iyon-core.md` | Read-only kernel and tool-contract inventory consumed by `tools/api-surface/` in T2. |
| `iyon-tui.md` | Read-only terminal/UI inventory consumed by `tools/api-surface/` in T2. |
| `iyon.md` | Read-only application and CLI inventory consumed by `tools/api-surface/` in T2 and the T10 app migration. |

Later `tools/api-surface/` work must give every reachable Rust capability a
documented and tested TypeScript path. Lazy façades count as coverage where
appropriate; parity does not require one-to-one N-API exports. There must be
no permanent missing or unsupported category. Existing public Rust paths and
behavior remain compatibility surfaces while bindings land.

The current-to-target ownership and compatibility matrix is maintained in
[`MIGRATION.md`](MIGRATION.md). It is the companion contract for module-level
sequencing and does not replace the four inventory baselines.

## Distribution and final executable

The TypeScript CLI in `packages/iyon-cli` is compiled by Bun into the canonical
standalone executable. Bun's standalone packaging embeds or packages the
`iyon-native.node` addon for the supported target. The executable starts Bun,
loads the runtime and ordinary TypeScript extensions, and reaches Rust only
through the documented `iyon:*` surfaces and native membrane.

The final product has no hidden native default registration path. The bundled
Iyon app, agent, providers, and tools are loaded and registered as ordinary
extensions through the same loader and registries available to external
packages.

## Migration boundary

Tranche 0 changes documentation only. It freezes ownership and compatibility
contracts before infrastructure or product code moves. T1 owns Bun/N-API
viability, T2 owns API scanning and SDK declarations, T3 owns kernel
extraction, T4/T5 own the real `iyon:*` bindings, T6 owns extension loading and
registries, T7–T10 own the bundled provider/tool/agent/app/CLI migrations, T11
owns post-parity bridge optimization, and T12 owns packaging and ecosystem
hardening.

No later-tranche implementation is required to make this document true at
T0. Rust behavior, manifests, inventories, tests, CI, and source layout remain
untouched until their owning tranche.

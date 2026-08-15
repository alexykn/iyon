# Iyon Rhai Refactor — End-to-End Implementation Handoff

**Planning baseline:** `alexykn/iyon` at `82903374f30faa27bf7dae31eba23e93dd54cb31`.

This plan treats the supplied API inventories as the compatibility contract. The objective is not merely “add Rhai plugins.” The end state is:

> **Iyon becomes a native Rust agent/runtime platform with a Rhai application and extension layer, while `iyon-api`, `iyon-core`, and `iyon-tui` remain first-class standalone Rust libraries. The shipped Iyon UX is implemented through the same public runtime surface available to third-party applications and plugins.**

The current `ARCHITECTURE.md` still describes the older `iyon-components` + WASM/WIT direction, so this refactor deliberately supersedes that architecture. 

---

# 1. Final architecture

## 1.1 Workspace

The final workspace should be:

```text
crates/
  iyon-api/
  iyon-core/
  iyon-tui/

  iyon-plugins/
    src/
      host/
      bindings/
      scheduler/
      registry/
      loader/
      operations/
      safety/
      testing/

  iyon/
    src/
      lib.rs
      tui/
      app_support/
      rhai_bindings.rs
      builtin.rs

    rhai/
      app/
        plugin.rhai
        app.rhai
        state.rhai
        conversation.rhai
        composer.rhai
        approvals.rhai
        theme.rhai
        tool_renderers/
          generic.rhai
          bash.rhai
          read.rhai
          write.rhai
          edit.rhai
          grep.rhai
          find.rhai
          ls.rhai

      providers/
        mock/
        openrouter/
        openai_codex/

      tools/
        bash/
        read/
        write/
        edit/
        grep/
        find/
        ls/

  iyon-cli/
    src/
      main.rs
      cli.rs
      run.rs
      auth/
      config.rs
      discovery.rs
      startup.rs

tools/
  api-surface/
    src/
      main.rs
      module_graph.rs
      visibility.rs
      reexports.rs
      impls.rs
      manifest.rs
```

I would **not create `iyon-components` in this refactor**. It is orthogonal to the runtime architecture and would increase the migration surface without helping the Rhai boundary.

## 1.2 Cargo dependency direction

```text
iyon-api
   ▲
   │
iyon-core

iyon-tui                 # independent of core/api

iyon-plugins
   ├── iyon-api
   ├── iyon-core
   ├── iyon-tui
   └── rhai

iyon
   ├── iyon-api
   ├── iyon-core
   ├── iyon-tui
   └── iyon-plugins

iyon-cli
   ├── iyon
   ├── iyon-api
   ├── iyon-core
   └── iyon-plugins
```

The hard rules are:

```text
iyon-api     ─X→ rhai
iyon-core    ─X→ rhai
iyon-tui     ─X→ rhai

iyon-tui     ─X→ iyon-core
iyon-tui     ─X→ iyon-api

iyon-plugins ─X→ iyon
```

`iyon-plugins` is the scripting host.

`iyon` is the reference application package plus compatibility facade.

`iyon-cli` owns the executable process.

---

# 2. Architectural decisions that are considered closed

These should be written into `ARCHITECTURE.md` as explicit invariants.

## 2.1 Rust remains the native API

Rhai does not define the architecture. It projects it.

Canonical correspondence:

```rust
View::text("hello")
    .bold()
    .padding(1)
```

becomes:

```rhai
view::text("hello")
    .bold()
    .padding(1)
```

Likewise:

```text
Rust                             Rhai

View::vertical                   view::vertical
DiffLine::context                diff_line::context
DiffRange::new                   diff_range::new
TextContent::raw                 text_content::raw
ReasoningLevel::next_for         reasoning_level::next_for
ModelError::unknown              model_error::unknown
History::push                    history.push
TextInput::set_text              input.set_text
Theme::set_style                 theme.set_style
```

Do not create an unrelated `tui::vertical` dialect.

Canonical package paths may still be available as:

```rhai
iyon_tui::view::vertical(...)
iyon_api::model_error::new(...)
iyon_core::core_command::submit_turn(...)
```

with the default Iyon prelude importing the short modules so application code uses:

```rhai
view::vertical(...)
core_command::submit_turn(...)
```

## 2.2 `Scene` and registries are separate

Do not turn `Scene` into a plugin registry or god object.

The current `Scene` is already a clean native abstraction:

```rust
pub struct Scene {
    history: Option<History>,
    body: View,
}
```

with mutable History access and `set_body()`. 

Its responsibility remains:

> **the semantic root presented to `iyon-tui`.**

Registries answer a different question:

> **which implementation currently owns a named capability?**

Thus:

```text
Scene
  body
  history

Plugin/Application host
  providers
  tools
  tool renderers
  commands
  scene composers
  connectors/services
  application factories
```

A plugin that wants to alter UI uses scene composition.

A plugin that wants to replace `bash` uses `ToolRegistry::replace`.

A completely different application does **not** overwrite the default Iyon app piece-by-piece. The CLI simply instantiates a different application factory and never instantiates `builtin:iyon`.

## 2.3 The shipped app is not privileged

Default startup:

```text
iyon-cli
   ↓
construct PluginHost
   ↓
load native binding packs
   ↓
load builtin contribution bundles
   ↓
resolve selected provider/tools
   ↓
spawn iyon-core
   ↓
instantiate application = builtin:iyon
   ↓
load extension scene composers / app extensions
   ↓
run iyon-tui
```

Alternative application:

```toml
[app]
entry = "local:my-agent"
```

then:

```text
iyon-cli
   ↓
same PluginHost
   ↓
same provider/tool/connector infrastructure
   ↓
same iyon-core
   ↓
instantiate local:my-agent
   ↓
same iyon-tui runtime
```

There must be no internal capability available to `builtin:iyon` that another application plugin cannot obtain through a public host API.

## 2.4 Rhai never owns asynchronous waiting

Rhai still has no suspendable interpreter stack / native language-level async execution. 

Therefore:

```text
Rhai = synchronous control plane
Tokio = asynchronous data plane
```

The prohibited steady-state architecture is:

```text
Rhai callback
   ↓
start HTTP
   ↓
block OS thread for 8 minutes
   ↓
continue Rhai
```

The required architecture is:

```text
short Rhai callback
   ↓
return/schedule HostOperation
   ↓
Rhai completely leaves the stack
   ↓
Tokio owns operation
   ↓
event becomes ready
   ↓
short Rhai callback
   ↓
return
```

That is what keeps 20, 50, 70 or more subagents from turning into 20–70 blocked scripting threads.

## 2.5 Preserve all existing TUI invariants

Do not modify the semantic pipeline:

```text
source/application state
  ↓
semantic content
  ↓
Renderer
  ↓
View
  ↓
layout/wrapping
  ↓
Theme/paint
  ↓
physical rows
  ↓
terminal
```

The current crate explicitly documents that architecture. 

In particular, the refactor must not touch the established native-History scrollback mechanism, semantic/source coordinates, streaming projection contracts, terminal model validation, or renderer/geometry boundary.

---

# 3. `iyon-api`: provider architecture

The current provider tree contains Mock, OpenAI Codex and OpenRouter implementations, while provider selection lives outside the crate. 

That is insufficient for runtime-loaded providers.

## 3.1 Add a native `ProviderRegistry`

Add:

```text
crates/iyon-api/src/provider_registry.rs
```

Recommended API:

```rust
pub type DynModelApi = Arc<dyn ModelApi>;

#[derive(Debug, Clone, Default)]
pub struct ModelCapabilities {
    pub reasoning_levels: Vec<ReasoningLevel>,
}

#[derive(Debug, Clone)]
pub struct ProviderDefinition {
    pub id: String,
    pub label: String,
}

pub trait ModelProviderFactory: Send + Sync {
    fn definition(&self) -> ProviderDefinition;

    fn capabilities(&self, model_id: &str) -> ModelCapabilities {
        ModelCapabilities::default()
    }

    fn create(
        &self,
        model_id: &str,
    ) -> Result<DynModelApi, ModelError>;
}

#[derive(Clone, Default)]
pub struct ProviderRegistry {
    providers: BTreeMap<String, Arc<dyn ModelProviderFactory>>,
}
```

Methods:

```rust
impl ProviderRegistry {
    pub fn new() -> Self;

    pub fn register(
        &mut self,
        provider: Arc<dyn ModelProviderFactory>,
    ) -> Result<(), ProviderRegistryError>;

    pub fn replace(
        &mut self,
        provider: Arc<dyn ModelProviderFactory>,
    ) -> Option<Arc<dyn ModelProviderFactory>>;

    pub fn remove(
        &mut self,
        id: &str,
    ) -> Option<Arc<dyn ModelProviderFactory>>;

    pub fn get(
        &self,
        id: &str,
    ) -> Option<Arc<dyn ModelProviderFactory>>;

    pub fn contains(&self, id: &str) -> bool;

    pub fn definitions(&self) -> Vec<ProviderDefinition>;

    pub fn create(
        &self,
        provider: &str,
        model_id: &str,
    ) -> Result<DynModelApi, ModelError>;

    pub fn capabilities(
        &self,
        provider: &str,
        model_id: &str,
    ) -> Option<ModelCapabilities>;
}
```

`register` must reject collisions.

`replace` must be explicit.

Never silently make “last plugin wins” the registry rule.

## 3.2 Keep all existing concrete providers public

These remain valid Rust APIs:

```text
MockModelApi
OpenAICodexModelApi
OpenRouterModelApi
```

Their current constructors and `ModelApi::stream` behavior remain intact.

The default executable can eventually dogfood the Rhai provider implementations, but removing the native provider types would violate the requested compatibility target.

## 3.3 Dynamic reasoning levels

The core must stop assuming that `ReasoningLevel::candidates(provider_string)` is the complete universe for runtime providers.

Keep the existing public method unchanged for compatibility.

For dynamically registered providers, use:

```rust
registry.capabilities(provider, model)
```

and pass the resulting supported reasoning levels into core configuration.

This means an Anthropic/Ollama/custom provider does not have to be hard-coded into `ReasoningLevel::candidates`.

---

# 4. `iyon-core`: make tools a genuine public extension API

There is already a strong tool extension design internally.

The current hidden implementation has:

```text
ToolDefinition
ToolExecutionMode
ToolApprovalPolicy
ToolSource
ToolExecutor
ToolContext
ToolResult
ToolUpdate
ToolUpdateSink
ToolRegistry
```

The existing registry already supports definitions, activation and model specs. 

The existing executor already has asynchronous execution, context, cancellation, progress and structured results. 

Do not replace this with another parallel “plugin tool” model.

## 4.1 Rename `ToolExecutor` → `Tool`

Because the trait is currently inaccessible externally, this is the right time to clean the name.

```rust
pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDefinition;

    fn execute(
        &self,
        ctx: ToolContext,
        input: serde_json::Value,
        updates: ToolUpdateSink,
    ) -> ToolFuture<'_>;
}
```

Then:

```rust
pub type DynTool = Arc<dyn Tool>;
```

## 4.2 Publicly expose the complete tool contract

`iyon_core::tools` should publicly export:

```text
Tool
DynTool
ToolFuture

ToolDefinition
ToolExecutionMode
ToolApprovalPolicy
ToolSource

ToolContext
ToolWorkspace
ToolResult

ToolUpdate
ToolUpdateSink

ToolRegistry

BeforeToolCallContext
BeforeToolCallDecision
BeforeToolCallResolution
BeforeToolCallHook
BeforeHookFuture

AfterToolCallContext
AfterToolCallPatch
AfterToolCallHook
AfterHookFuture

ToolHookSet
ToolHookSnapshot
```

The current definition model already contains execution mode, approval, source, prompt snippet and guidelines. 

## 4.3 Introduce `ToolWorkspace`

Do **not** expose arbitrary raw filesystem capability just because `ToolContext` becomes public.

The current internal `Workspace` already implements root-safe path resolution and read/write policy. 

Wrap that as:

```rust
#[derive(Clone)]
pub struct ToolWorkspace {
    inner: Workspace,
}
```

Public:

```rust
impl ToolWorkspace {
    pub fn root(&self) -> &Path;

    pub fn resolve_read_path(
        &self,
        path: &str,
    ) -> anyhow::Result<PathBuf>;

    pub fn resolve_write_path(
        &self,
        path: &str,
    ) -> anyhow::Result<PathBuf>;

    pub fn resolve_search_path(
        &self,
        path: Option<&str>,
    ) -> anyhow::Result<PathBuf>;

    pub fn ensure_read_allowed(
        &self,
        path: &Path,
    ) -> anyhow::Result<()>;

    pub fn ensure_write_allowed(
        &self,
        path: &Path,
    ) -> anyhow::Result<()>;
}
```

No public unrestricted constructor is required. Core supplies the capability.

Then:

```rust
pub struct ToolContext {
    pub session_id: SessionId,
    pub turn_id: TurnId,
    pub tool_call_id: ToolCallId,
    pub cwd: PathBuf,
    pub workspace: ToolWorkspace,
    pub cancellation: CancellationToken,
}
```

## 4.4 Promote and extend `ToolRegistry`

Keep:

```text
new
register
get
definitions
active_definitions
set_active_tools
active_tool_names
model_specs
```

Add:

```rust
pub fn replace(
    &mut self,
    tool: DynTool,
) -> Option<DynTool>;

pub fn remove(
    &mut self,
    name: &str,
) -> Option<DynTool>;

pub fn contains(
    &self,
    name: &str,
) -> bool;

pub fn names(
    &self,
) -> Vec<String>;
```

Make built-in construction cleaner:

```rust
pub fn builtin_defaults() -> anyhow::Result<Self>;

pub fn register_builtin_defaults(
    &mut self,
) -> anyhow::Result<()>;

pub fn register_read_only_defaults(
    &mut self,
) -> anyhow::Result<()>;
```

The shared file-mutation queue should be created internally by the default registration routine rather than becoming a public parameter.

## 4.5 Core must accept a ToolRegistry

Today `RuntimeState::new` hardcodes construction of the built-in registry. 

Replace that with injected runtime configuration.

Add:

```rust
pub struct IyonCoreBuilder {
    model: Arc<dyn ModelApi>,
    selection: ModelSelection,
    reasoning_levels: Vec<ReasoningLevel>,
    tools: ToolRegistry,
    tool_hooks: ToolHookSet,
}
```

Suggested API:

```rust
impl IyonCoreBuilder {
    pub fn new(model: Arc<dyn ModelApi>) -> anyhow::Result<Self>;

    pub fn selection(
        self,
        selection: ModelSelection,
    ) -> Self;

    pub fn reasoning_levels(
        self,
        levels: impl IntoIterator<Item = ReasoningLevel>,
    ) -> Self;

    pub fn tool_registry(
        self,
        tools: ToolRegistry,
    ) -> Self;

    pub fn tool_hooks(
        self,
        hooks: ToolHookSet,
    ) -> Self;

    pub fn spawn_with_owned_runtime(
        self,
    ) -> anyhow::Result<IyonCore>;

    pub fn spawn_on_current_runtime(
        self,
    ) -> IyonCore;
}
```

**All current `IyonCore::spawn*` methods remain.**

They become convenience wrappers around `IyonCoreBuilder`.

This avoids breaking Rust consumers while preventing another constructor explosion.

---

# 5. `iyon-tui`: minimal additions only

The TUI should not learn what Rhai or plugins are.

The existing `App` is generic and stores `init`, `update`, `view`, History and Theme. 

The kernel already owns a real `Scene`; it sets the body after initialization and regenerates it when application state becomes dirty.  

That gives us the exact place to add composition.

## 5.1 Add render-phase Scene composition

Add a generic, plugin-unaware abstraction:

```rust
pub trait SceneComposer<State>: 'static {
    fn compose(
        &mut self,
        state: &State,
        scene: &mut Scene,
    );
}
```

And:

```rust
impl<...> App<...> {
    pub fn with_scene_composer<C>(
        self,
        composer: C,
    ) -> Self
    where
        C: SceneComposer<State>;
}
```

Internally the frame becomes:

```text
state
  ↓
ViewFn
  ↓
scene.set_body(base)
  ↓
SceneComposer #1
  ↓
SceneComposer #2
  ↓
...
  ↓
SceneHost
  ↓
layout / paint / terminal
```

The critical semantic rule is:

> A SceneComposer transforms the freshly derived scene for the frame. It must be idempotent and must not use rendering as a place to perform application side effects.

Normal History mutations continue to occur in update/event handling through `AppCx`.

This makes UI extension deterministic: composition cannot accidentally accumulate wrappers between frames because the base body is recreated first.

## 5.2 Improve body replacement ergonomics

Add:

```rust
pub fn replace_body(
    &mut self,
    body: impl IntoView,
) -> View;
```

Potentially also:

```rust
pub fn map_body(
    &mut self,
    map: impl FnOnce(View) -> View,
);
```

These are generic `Scene` APIs, not plugin APIs.

A scene extension can then do the conceptual equivalent of:

```rhai
fn compose(scene) {
    let old = scene.replace_body(view::spacer(0));

    scene.set_body(
        view::horizontal([
            my_sidebar(),
            old,
        ])
    );
}
```

## 5.3 Do not add arbitrary node lookup

Do **not** add:

```text
scene.find("composer")
scene.find("model_picker")
```

to `iyon-tui`.

Those identities are application concepts.

If `builtin:iyon` intentionally wants a replaceable composer/footer/model-picker slot, that should be an application-level contribution registry in `iyon`, not semantic knowledge inside generic `Scene`.

---

# 6. The Rhai binding architecture

## 6.1 Native values, not JSON FFI

Rhai custom types can carry Rust types directly; under the `sync` feature they need the corresponding thread-safe custom-type properties. 

Therefore:

```text
View
HistoryUnitId
DiffHunk
ToolCallId
ModelRequest
CoreEvent
Theme
TextContent
...
```

remain native in-memory Rust values or small native wrappers.

Do not serialize View trees to JSON to cross the scripting boundary.

## 6.2 Binding packages

Create:

```text
iyon-plugins/src/bindings/
  mod.rs
  manifest.rs
  registrar.rs
  value.rs
  error.rs
  borrowed.rs

  api/
    mod.rs
    model.rs
    error.rs
    stream.rs
    providers.rs
    registry.rs

  core/
    mod.rs
    command.rs
    event.rs
    handle.rs
    ids.rs
    session.rs

    tools/
      mod.rs
      definition.rs
      executor.rs
      registry.rs
      hooks.rs
      output.rs

  tui/
    mod.rs
    application.rs
    input.rs
    output.rs
    component.rs
    scene.rs
    view.rs
    style.rs
    theme.rs
    history.rs
    diff.rs

    projection/
    stream/
    text/
    testing.rs
```

## 6.3 No ad-hoc binding registration

Prohibit direct calls scattered through the codebase like:

```rust
engine.register_fn(...);
```

Every binding must be declared through a single binding system.

Conceptually:

```rust
bind_method! {
    rust = "iyon_tui::View::padding",
    rhai = "iyon_tui::view.padding",
    type = View,
    fn = View::padding,
}
```

Each declaration generates:

1. the actual Rhai registration,
2. a `BindingDescriptor`,
3. API coverage metadata,
4. generated documentation metadata.

Representative descriptor:

```rust
pub struct BindingDescriptor {
    pub rust_path: &'static str,
    pub rhai_path: &'static str,
    pub kind: BindingKind,
    pub feature: Option<&'static str>,
    pub alias_of: Option<&'static str>,
}
```

`BindingKind` should include:

```text
Type
TypeAlias
Constructor
AssociatedFunction
Method
FieldGetter
FieldSetter
EnumVariant
EnumVariantConstructor
AssociatedConst
FreeFunction
TraitAdapter
GenericErasure
IteratorSnapshot
ScopedBorrow
AsyncOperation
OpaqueHandle
Alias
TestOnly
```

There is deliberately **no permanent `Unsupported` or `Ignore` category**.

## 6.4 `iyon` bindings without a dependency cycle

`iyon-plugins` cannot depend on `iyon`.

Therefore app-specific bindings live in:

```text
crates/iyon/src/rhai_bindings.rs
```

but `iyon` must not need to understand raw Rhai internals.

Expose from `iyon-plugins`:

```rust
pub trait BindingPack {
    fn id(&self) -> &'static str;

    fn register(
        &self,
        registrar: &mut BindingRegistrar,
    ) -> Result<(), BindingError>;

    fn descriptors(
        &self,
    ) -> &'static [BindingDescriptor];
}
```

The registrar/macros hide Rhai through `$crate` internals.

So `iyon` can register:

```text
ToolDraftKey
FrontendEvent
ToolUpdatePresentation
IyonAction
ApprovalDecision
IyonState
```

without importing `rhai::Engine`.

---

# 7. Rhai type-conversion rules

These rules must be documented and applied uniformly.

| Rust API type | Rhai representation |
|---|---|
| `String`, `&str`, `Arc<str>` | Rhai string |
| `bool`, integer primitives, floats | native scalar |
| `Option<T>` | `()` for `None`, mapped value for `Some` |
| `Result<T, E>` | return mapped `T`; `Err` becomes typed script exception |
| `serde_json::Value` | recursive Rhai scalar/Array/Map |
| `Vec<u8>` | Rhai blob |
| `Vec<T>` / slices | Array snapshot unless mutation is explicitly supported |
| iterators | Rhai iterator/Array snapshot |
| `PathBuf` | path string at script surface |
| `Duration` | host `Duration` custom value |
| `Instant` | opaque host `Instant` |
| IDs | native opaque/custom type |
| `View`, semantic IR | native custom values |
| `Arc<dyn Trait>` | host wrapper around native or Rhai-backed implementation |
| generic `Output<T>` | `Output<Dynamic>` script specialization |
| generic `Projection<T>` | `Projection<Dynamic>` script specialization |
| `ComponentHandle<C>` | erased/specialized script component handle |
| `HistoryStreamHandle<S>` | Rhai-stream-source specialized handle |

`Result` being mapped to exceptions does **not** mean error types disappear. Public error enums remain registered custom types and the thrown host error carries the typed cause.

---

# 8. Rust lifetimes and borrowed APIs

This is one of the places implementation needs discipline.

Never put a Rust borrowed reference into `Dynamic` and hope the script does not keep it.

There are two patterns.

## Read-only contexts become owned snapshots

For:

```text
BeforeToolCallContext<'a>
AfterToolCallContext<'a>
TextChange<'a>
```

construct an owned script-facing snapshot containing the same observable public values.

## Mutable callback contexts use scoped handles

`AppCx<'a, Action>` cannot be copied.

Expose an ephemeral:

```rust
struct ScopedAppCx {
    scope_id: CallbackScopeId,
}
```

During callback execution:

```text
enter callback scope #41
     ↓
make ScopedAppCx(41)
     ↓
execute Rhai
     ↓
native method resolves scope 41 → current real AppCx
     ↓
script returns
     ↓
invalidate scope 41
```

If the script stores the value and later calls it:

```text
PluginError::ExpiredScope
```

Apply the same pattern anywhere a mutable native borrowed capability must remain live only for one callback.

Keep this machinery inside a tiny audited subsystem:

```text
bindings/borrowed.rs
```

and test escape/use-after-callback aggressively.

---

# 9. Generic/trait APIs in Rhai

The user-supplied API inventory contains substantial generic APIs. “Complete coverage” must not mean only binding the easy structs.

## `Component`

Script component:

```rhai
component::define(#{
    view: Fn("view"),
    capabilities: Fn("capabilities"),
})
```

becomes a concrete native `RhaiComponent` implementing:

```rust
iyon_tui::Component
```

## `Renderer`

```rhai
renderer::define(Fn("render"))
```

becomes `RhaiRenderer`.

## `StreamingSource`

```rhai
streaming_source::define(#{
    snapshot: Fn("snapshot"),
    seal: Fn("seal"),
    is_sealed: Fn("is_sealed"),
    compact_before: Fn("compact_before"),
    advance: Fn("advance"),
})
```

becomes concrete `RhaiStreamingSource: StreamingSource`.

That enables native:

```text
History::push_stream<RhaiStreamingSource>
StreamPane<RhaiStreamingSource>
HistoryStreamHandle<RhaiStreamingSource>
```

without weakening `iyon-tui`'s type system.

## `Projector`

Rhai gets `Projection<Dynamic>` and a `RhaiProjector`.

`project`, `restart_from`, `next_wakeup`, `advance`, composition and projection validation must all be represented.

## `TextRewriter` / `TextVisitor`

Provide concrete callback-backed adapter implementations.

## Core hooks

`RhaiBeforeToolCallHook` and `RhaiAfterToolCallHook` implement the native async hook traits.

The Rust future waits on the **host scheduler**, not on a blocking Rhai invocation.

---

# 10. Asynchronous operation scheduler

This is the core of `iyon-plugins`.

## 10.1 Logical model

```rust
pub struct PluginTaskId(u64);
pub struct OperationId(u64);

pub enum HostOperation {
    Http(HttpRequest),
    Process(ProcessRequest),
    Fs(FsOperation),
    Timer(TimerOperation),
    Core(CoreOperation),
    Connector(ConnectorOperation),
}

pub enum OperationEvent {
    Started {
        operation_id: OperationId,
    },

    Data {
        operation_id: OperationId,
        data: HostValue,
    },

    Progress {
        operation_id: OperationId,
        value: HostValue,
    },

    Completed {
        operation_id: OperationId,
        value: HostValue,
    },

    Failed {
        operation_id: OperationId,
        error: HostOperationError,
    },

    Cancelled {
        operation_id: OperationId,
    },
}
```

A script callback returns something conceptually equivalent to:

```rust
pub enum PluginDecision {
    Continue,
    Start(Vec<HostOperation>),
    Complete(HostValue),
    Fail(PluginError),
}
```

## 10.2 Execution strand

Each stateful logical script task has an ordered execution strand.

```text
provider stream #12
  event queue
       ↓
callback 1
       ↓
callback 2
       ↓
callback 3
```

Never invoke `on_event` concurrently for the same provider stream/tool execution unless that script explicitly opts into it later.

Different tasks may execute concurrently up to the worker limit.

## 10.3 Worker pool

Configure a bounded pool, for example:

```text
Rhai callback workers = min(available_parallelism, configured maximum)
```

Do **not** create one worker per subagent.

With:

```text
70 agents
40 pending tool operations
70 model streams
```

the expected architecture is:

```text
110 logical tasks
    │
    ├── mostly sleeping as Tokio futures
    │
    └── short callback bursts
             │
      bounded Rhai worker pool
```

rather than 110 blocked OS threads.

## 10.4 Backpressure

Every bridge channel must be bounded.

For a provider:

```text
HTTP/SSE
   ↓ bounded
transport events
   ↓
provider strand
   ↓ bounded
ModelStreamEvent
   ↓
iyon-core
```

If the script cannot process incoming stream events fast enough, transport naturally stops reading ahead instead of allocating indefinitely.

## 10.5 Cancellation

Each logical task owns a child `CancellationToken`.

Cancellation must:

1. stop pending host operations,
2. stop accepting new events,
3. invalidate queued callback invocations,
4. perform at most one final cancellation callback if specified,
5. release task state,
6. cause the appropriate native `Cancelled` result/event.

Do not implement cancellation merely by ignoring the eventual result.

---

# 11. Rhai providers

The provider should own **protocol semantics**, not sockets.

Example structure:

```rhai
fn definition() {
    #{
        id: "openrouter",
        label: "OpenRouter",
    }
}

fn capabilities(model_id) {
    #{
        reasoning_levels: [...],
    }
}

fn request(model_id, request, cx) {
    http_request::post("https://...")
        .header(...)
        .json(build_body(model_id, request))
}

fn on_sse(state, event, cx) {
    // provider-specific event interpretation

    [
        model_stream_event::text_delta(...),
        ...
    ]
}
```

Rust host owns:

```text
DNS
TCP
TLS
HTTP
timeouts
cancellation
SSE framing
backpressure
connection errors
```

Rhai owns:

```text
headers/body shape
provider JSON
provider event names
tool-call reconstruction rules
usage mapping
stop-reason mapping
provider quirks
```

This means a ten-minute model stream occupies a Rhai worker only for small request/event callbacks.

---

# 12. Rhai tools

A script tool should register execution and presentation together:

```rhai
fn activate(host) {
    host.tools.register(
        tool::define(#{
            definition: tool_definition::new(...),
            execute: Fn("execute"),
            on_event: Fn("on_event"),
            render_call: Fn("render_call"),
            render_result: Fn("render_result"),
        })
    );
}
```

A read tool might:

```rhai
fn execute(args, cx) {
    cx.start(
        fs::read_text(args.path)
    )
}

fn on_event(state, event, cx) {
    if event.is_completed {
        return tool::complete(event.value);
    }

    tool::continue()
}
```

There is no sleeping scripting thread.

Pure synchronous tools remain possible:

```rhai
fn execute(args, cx) {
    tool::complete(args.text.trim())
}
```

---

# 13. Plugin/application registries

`iyon-plugins` should own contribution identity.

```rust
pub struct ContributionId {
    pub plugin: PluginId,
    pub kind: ContributionKind,
    pub name: String,
}
```

Registries include:

```text
ApplicationRegistry
ProviderRegistry adapter
ToolRegistry adapter
ToolRendererRegistry
CommandRegistry
SceneComposerRegistry
Connector/ServiceRegistry
```

Common semantics:

```text
register(name, value)
  duplicate -> error

replace(name, value)
  explicit replacement

remove(name)

get(name)

owner(name)
```

A plugin never reaches into another plugin's private Rhai state.

---

# 14. Loading and configuration

Use a custom Rhai `ModuleResolver`; Rhai explicitly supports customizable module resolution. 

Search order:

```text
builtin:
~/.config/iyon/plugins/
<workspace>/.iyon/plugins/
explicit configured paths
```

Configuration:

```toml
[app]
entry = "builtin:iyon"

[plugins]
paths = []
enabled = []
disabled = []
```

Recommended package:

```text
my-plugin/
  plugin.toml
  plugin.rhai
  modules/
```

`plugin.toml`:

```toml
id = "com.example.foo"
entry = "plugin.rhai"
```

Do not build dependency solving, marketplace negotiation or hot reload in this refactor.

---

# 15. Plugin activation must be transactional

Loading sequence:

```text
discover
   ↓
read manifest
   ↓
compile Rhai AST
   ↓
validate exported entry points
   ↓
begin ContributionTransaction
   ↓
activate/register
   ↓
validate registry invariants
   ↓
commit
```

If activation fails:

```text
rollback every contribution from this activation
```

The host must never be left with:

```text
provider registered
tool renderer missing
keybinding half-installed
scene composer installed
tool absent
```

from a failed plugin.

---

# 16. Security model

Do not describe Rhai plugins as sandboxed in the WASM sense.

The security boundary is:

> a script can do only what the host exposes to it, but the interpreter itself is in-process trusted code execution.

Use:

```text
custom restricted ModuleResolver
max operations
max call depth
max expression depth
max variables
max functions
max modules
max array/map/string size
progress callback / cancellation
bounded host-operation concurrency
bounded output channels
```

Rhai exposes execution-operation and progress limits for exactly this kind of guardrail. 

---

# 17. Preserve `iyon` as a library compatibility facade

Do **not** delete the `iyon` crate.

Move the binary out, but keep:

```text
iyon::tui
```

and its current public API.

This matters because the user-supplied API inventory explicitly includes it.

## 17.1 `IyonState`

Today `IyonState` is public but its state is private. That is ideal for migration.

Its implementation can become:

```rust
pub struct IyonState {
    runtime: RhaiApplicationState,
}
```

without exposing a source-level break.

## 17.2 `build_app`

Preserve:

```rust
iyon::tui::build_app(...)
```

Its closures become adapters:

```text
init
  ↓
instantiate builtin Rhai application

update
  ↓
dispatch IyonAction into Rhai

view
  ↓
call Rhai application view
  ↓
native View
```

Its concrete closure types are already hidden behind `impl Fn*`, so implementation details can change.

Add an advanced constructor:

```rust
pub fn build_app_with_host(
    host: ApplicationHost,
    command_tx: CoreCommandSender,
    selection: ModelSelection,
) -> ...
```

`build_app` remains the convenient builtin-Iyon wrapper.

## 17.3 `run_with_core`

Keep current behavior and public signature.

The current implementation splits core, creates the app, starts the core bridge, runs and cleanly shuts down the bridge. 

Internals may change; observable behavior must not.

---

# 18. Keep the CoreEvent mapper native

I would **not** move the current `CoreEventMapper` and coalescer into Rhai.

It is transport adaptation, not application presentation.

Today it:

- tracks message role,
- turns ToolCall Start/Arguments/End into Preparing/Arguments/Prepared,
- preserves `{message_id, content_index}`,
- coalesces adjacent assistant/thinking deltas,
- coalesces argument deltas only for the same draft,
- replaces same-kind progress/detail snapshots,
- asynchronously forwards mapped events to the app.  

That logic is precisely the sort of stable native adapter that Rhai applications should consume.

The Rhai application sees native `FrontendEvent`.

This also protects the carefully established distinction between:

```text
model-generation tool lifecycle
```

and:

```text
actual tool execution lifecycle
```

---

# 19. Keep high-risk streaming machinery native

The current application has substantial native transcript machinery:

```text
transcript/
  assistant_stream/
  pipe_table.rs
  pipeline.rs
  semantic.rs
```



Do not rewrite this algorithm in Rhai just to maximize line-count dogfooding.

`AssistantStream` is an application-specific **native support type** exposed to the Rhai app.

Rhai controls:

```text
when stream starts
when deltas are appended
when stream seals
where it sits in History
which semantic segment is appended
```

Native helper controls:

```text
Markdown projection
source provenance
pacing
Smooth
pipe-table stabilization
stream snapshots
```

Likewise, keep the current unified-diff parser native application support and let Rhai render its semantic `DiffHunk` values through the public `DiffRenderer`.

This preserves the architectural boundary: parsing is still application-side; rendering remains generic `iyon-tui`.

---

# 20. Exact current application behavior to preserve

The port must preserve, rather than approximately recreate, the current behavior in `state.rs`.  

That includes:

```text
MAX_COMPOSER_ROWS = 13

large paste:
  > 1000 characters OR > 10 lines
  → [Pasted Content N chars]

paste normalization:
  CRLF → LF
  CR   → LF
  tab  → four spaces

Ctrl+C:
  composer non-empty → clear composer
  else active turn   → cancel turn
  else               → Goodbye + exit

Escape:
  active turn → cancel

Shift+Tab:
  cycle reasoning effort

composer:
  multiline
  top/bottom border
  input.border theme color

footer:
  provider · model · effort · status
```

And conversation lifecycle:

```text
user batch
  ↓
working state
  ↓
assistant stream
  ↓
tool draft
  ↓
prepared
  ↓
running
  ↓
approval if required
  ↓
result
  ↓
freeze
```

The existing implementation intentionally maintains a separate working component so the tool card pulse cannot restart its animation. Preserve that effect, not merely the textual output. 

---

# 21. API drift prevention

This is mandatory and should become one of the strongest parts of the project.

## 21.1 Stable source scanner

Build:

```text
tools/api-surface
```

using:

```text
syn
cargo_metadata
```

It must calculate the **externally reachable source API**, not merely grep for `pub`.

It has to understand:

```text
public modules
private modules
pub use through private modules
nested pub use
glob reexports
struct public fields
enum variants + variant fields
type aliases
free functions
consts/statics
traits
associated types
associated constants
trait methods
inherent impl methods
trait impl projections
cfg / features
```

This matters because the existing crates deliberately expose public definitions from private implementation modules.

The scanner should represent paths like:

```text
iyon_tui::View
iyon_tui::View::padding
iyon_tui::text::TextRun::split_at
iyon_api::ModelRequest::system_prompt
iyon_core::CoreCommand::RejectToolCall::reason
```

## 21.2 Preserve every public alias

If a type is reachable both as:

```text
iyon_tui::TextContent
iyon_tui::text::TextContent
```

both paths are part of the API graph.

One binding may be canonical and another an alias, but the manifest must account for both.

## 21.3 Compiler-truth nightly check

Rustdoc JSON remains experimental and requires the unstable/nightly path, so it is not appropriate as the sole ordinary `cargo test` dependency. 

Use two layers:

```text
Every PR / stable:
  tools/api-surface
       ↕
  binding manifest

Nightly CI:
  rustdoc JSON
       ↕
  tools/api-surface
       ↕
  binding manifest
```

The nightly job detects mistakes in the custom parser.

## 21.4 Mandatory equality test

After the migration:

```rust
assert_eq!(
    reachable_public_rust_api(),
    binding_manifest.rust_paths(),
);
```

With alias normalization where explicitly declared.

No permanent exception file.

No:

```text
ignored because hard
not relevant to Rhai
internal-ish
```

for something Rust exposes publicly.

## 21.5 Migration-only debt file

During the first two binding tranches only, create:

```text
api/rhai-unbound-baseline.toml
```

containing the API that existed at `82903374...` but is not bound yet.

Rules:

```text
new Rust public API not in binding manifest
AND not in original baseline
→ CI failure

bound old API
→ remove baseline entry

end of binding tranche
→ baseline must be empty and file deleted
```

After that, coverage becomes strict forever.

## 21.6 Generated ledger

CI should generate:

```text
docs/generated/RHAI_API_COVERAGE.md
target/api-surface/iyon-api.json
target/api-surface/iyon-core.json
target/api-surface/iyon-tui.json
target/api-surface/iyon.json
```

Never hand-maintain the coverage document.

---

# 22. Coverage rules for the supplied APIs

The entire enumeration in the prompt is the starting compatibility baseline.

The following rules tell the implementation agent how every category must map.

## `iyon-api`

Every one of these families must be represented:

```text
ModelStream
ModelStreamFuture
ModelApi

ModelError
ModelErrorKind

ModelRequest
ModelParams
ModelMetadata
ReasoningLevel
CacheRetention
ModelMessage
ContentBlock
ModelToolSpec

ModelStreamEvent
Usage
StopReason

MockModelApi
OpenAICodexModelApi
OpenRouterModelApi
```

Every listed field, variant, associated constant, constructor and method is a manifest item.

For async stream/future aliases, use `BindingKind::AsyncOperation`; do not pretend they are synchronous script values.

## `iyon-core`

Coverage includes:

```text
CoreCommand
CoreEvent
MessageDelta
MessageRole
ToolCallDelta
ToolUpdateEvent

IyonCore
CoreCommandSender
CoreEventReceiver

all five ID types

ModelSelection
SessionState

all public hooks
all hook contexts
all hook decisions/resolutions
ToolHookSet
ToolHookSnapshot

all output truncation enums/structs/constants/functions
```

plus all newly public tool-extension APIs.

Explicit blocking methods such as:

```text
blocking_recv_event
```

remain available by name but must only be callable from a background scripting execution context. Calling one from the UI/app executor should produce a host error rather than stall presentation.

## `iyon-tui`

Coverage must include the complete supplied inventory, including the advanced pieces:

```text
App / AppCx / AppHandle / App errors
Component / ComponentCx / ComponentHandle
Output / OutputRouter / EventCx
Scene
View / Text
Vertical / Horizontal / Grid
styles / colors / borders / theme
TextInput
ScrollPane
History
Diff
Projection
Projector
Smooth
Stream
StreamingSource
Text semantic IR
Markdown
text visitors and rewriters
testing::AppHarness
```

Do not stop at `View`, `TextInput`, and `Theme`.

The projection/stream/text compiler APIs are explicitly public and therefore explicitly part of script parity.

## `iyon`

Coverage includes:

```text
build_app
run_with_core

ToolDraftKey
ApprovalDecision
IyonState

FrontendEvent
ToolUpdatePresentation
IyonAction
```

`IyonState` and `ApprovalDecision` may remain opaque where their Rust API is opaque.

Fields that are Rust-public must be available as script properties.

---

# 23. Existing end-to-end tests become the UX oracle

The current `crates/iyon/tests/public_app.rs` is extremely valuable.

It already tests the application through `iyon-tui::testing` rather than its internals. It covers composer/paste, smoothing, tool lifecycle, History layout and exit behavior. 

Do not rewrite those tests around implementation details.

During migration create:

```rust
enum AppImplementation {
    LegacyRust,
    Rhai,
}
```

and parameterize the scenario runner.

Conceptually:

```rust
run_public_app_suite(AppImplementation::LegacyRust);
run_public_app_suite(AppImplementation::Rhai);
```

For deterministic scenarios additionally compare:

```text
screen_lines()
native_history_lines()
exit state
registered activity count if observable
```

The legacy implementation can only be removed when the Rhai path passes the entire suite.

---

# 24. Tranche 0 — Architecture contract and public-API drift infrastructure

**One ambitious merge request. No product behavior change.**

## Scope

Create the structural foundation before changing runtime behavior.

### Add

```text
crates/iyon-plugins/
tools/api-surface/
```

Optionally scaffold `crates/iyon-cli/` as a library-only package, but do not switch the executable yet.

### Rewrite

```text
ARCHITECTURE.md
```

Remove the WASM/WIT and `iyon-components` planned architecture from the active design.

Document:

```text
native Rust layers
Rhai host
app selection
Scene vs registry
event-driven async
binding parity
security model
non-goals
```

### Build API scanner

Generate the exact current surface from the source.

Seed:

```text
api/rhai-unbound-baseline.toml
```

from commit `82903374...`.

### CI

Add:

```text
cargo run -p api-surface -- check
cargo test -p iyon-plugins --test api_surface
```

and nightly rustdoc JSON cross-check.

## Tests

- private-parent/public-reexport fixture
- glob reexport fixture
- duplicate alias fixture
- inherent method fixture
- trait method implementation fixture
- enum field fixture
- feature-gated API fixture
- `test-util` fixture
- scanner output matches the supplied inventories

## Completion criteria

- Existing workspace tests unchanged and green.
- No application/runtime behavior changed.
- Exact public API baseline is machine-readable.
- New public APIs cannot accidentally bypass Rhai coverage planning.
- Architecture document describes only the new architecture.

---

# 25. Tranche 1 — Native provider and tool extension surfaces

**One merge request. Entirely useful to Rust consumers even without Rhai.**

## `iyon-api`

Add:

```text
provider_registry.rs
ProviderDefinition
ModelCapabilities
ModelProviderFactory
ProviderRegistry
ProviderRegistryError
```

Export all from `lib.rs`.

Do not modify current provider constructors or stream semantics.

## `iyon-core`

Refactor:

```text
tools/definition.rs
tools/executor.rs
tools/registry.rs
tools/mod.rs
runtime.rs
handle.rs
lib.rs
```

### Changes

- `ToolExecutor` → public `Tool`
- promote tool types
- introduce `ToolWorkspace`
- promote ToolRegistry
- add explicit replace/remove
- inject ToolRegistry into runtime
- add `IyonCoreBuilder`
- existing constructors become builder wrappers
- dynamic reasoning capabilities enter RuntimeConfig

The current RuntimeState creates built-ins directly; that hardcoded point is removed. 

## Tests

### Registry

```text
register succeeds
duplicate register fails
replace replaces exactly once
remove
active tool behavior
model_specs
clone snapshots registry correctly
```

### Core

Run the same core scenario twice:

```text
default constructors
builder with equivalent builtin registry
```

and assert identical CoreEvent sequence.

### Custom Rust tool

Integration test in an external-style test module:

```rust
struct TestTool;
impl iyon_core::tools::Tool for TestTool { ... }
```

Inject it through builder, have model invoke it, verify:

```text
Prepared
Started
Updated
Finished
Result
```

### Custom Rust provider

Register a factory with `ProviderRegistry`, resolve it and run a core turn.

## Completion criteria

- A separate Rust application can define a provider and tool without private APIs.
- Existing public constructors still compile.
- Existing current behavior remains identical.
- No Rhai is involved yet.

---

# 26. Tranche 2 — Complete Rhai binding surface

**This is intentionally a large merge request.**

At its end, the migration baseline is empty.

## Implement base host

```text
PluginEngine
BindingRegistrar
BindingManifest
HostValue
typed error bridge
JSON bridge
time/path bridge
scoped-borrow bridge
custom native-value wrappers
```

Enable Rhai's thread-safe mode for the plugin host.

## Bind `iyon-api`

Everything in the user's inventory.

## Bind `iyon-core`

Everything in the inventory plus Tranche-1 APIs.

## Bind `iyon-tui`

Everything in the inventory, including:

```text
projection
stream
text IR
visitors/rewriters
testing
```

## Bind `iyon`

Implement its `BindingPack` in `crates/iyon/src/rhai_bindings.rs`.

## Naming tests

For every descriptor:

```text
Rust Type::method
    ↓ deterministic transform
expected Rhai type_module::method
```

Manual aliases require an explicit descriptor.

This lets CI catch:

```rust
View::padding
```

being accidentally exposed as:

```text
view::pad
```

## Representative executable smoke tests

Not just registry introspection.

Execute Rhai snippets for every binding class.

Examples:

```rhai
let v =
    view::vertical([
        view::text("hello").bold(),
        view::text("world").dim(),
    ])
    .padding(insets::all(1));
```

```rhai
let r = diff_range::new(
    diff_line_offset::new(0),
    1
);
```

```rhai
let h = history::new();
```

```rhai
let opts =
    markdown_options::gfm()
        .with_tables(true);
```

## Completion criteria

```text
reachable Rust public API
MINUS
binding manifest
=
empty
```

Delete:

```text
api/rhai-unbound-baseline.toml
```

From this merge onward, missing parity is a hard CI failure.

---

# 27. Tranche 3 — Scalable async operation host and Rhai Tool/Provider adapters

This tranche proves that the architecture scales beyond the simple blocking bridge.

## Implement

```text
scheduler/
  task.rs
  strand.rs
  worker_pool.rs
  queue.rs
  cancellation.rs
  metrics.rs

operations/
  mod.rs
  http.rs
  sse.rs
  process.rs
  fs.rs
  timer.rs
```

## Rhai provider adapter

```rust
RhaiModelProviderFactory
RhaiModelApi
```

The resulting `RhaiModelApi` is an ordinary:

```rust
Arc<dyn iyon_api::ModelApi>
```

to the core.

## Rhai tool adapter

```rust
RhaiTool: iyon_core::tools::Tool
```

The resulting tool is an ordinary native `DynTool`.

Core does not know the implementation is a script.

## Hooks

Add:

```text
RhaiBeforeToolCallHook
RhaiAfterToolCallHook
```

## Test provider

Create a fully scripted provider that uses a fake asynchronous transport driver:

```text
request
wait
TextStart
TextDelta
TextEnd
Usage
Done
```

## Test tool

Create a tool that:

```text
starts operation
receives progress
receives completion
returns result
```

## Scaling test

Run at least:

```text
100 logical model streams
100 logical tool waits
```

with a deliberately small callback worker count.

Instrument the scheduler and assert:

```text
max simultaneously executing Rhai callbacks <= configured worker count
logical task count can exceed worker count
pending Tokio operations do not reserve Rhai workers
event ordering per strand is stable
```

On Linux CI, an additional `/proc/self/status` thread-count assertion is worthwhile, but the portable scheduler instrumentation is the authoritative test.

## Cancellation stress

Cancel operations at:

```text
before request start
while queued for Rhai worker
during HTTP wait
between stream chunks
during tool process
after host completion but before callback
```

Each logical activity must produce exactly one terminal outcome.

## Completion criteria

No architecture in this tranche may use:

```text
one OS thread per logical provider stream
one OS thread per logical tool execution
```

---

# 28. Tranche 4 — Plugin lifecycle, application registry and Scene composition

This makes Iyon genuinely replaceable.

## Registries

Implement:

```text
ApplicationRegistry
ToolRendererRegistry
CommandRegistry
SceneComposerRegistry
ServiceRegistry
```

Wrap `iyon-api::ProviderRegistry` and `iyon-core::ToolRegistry` with plugin contribution ownership rather than duplicating their functionality.

## Application contract

Conceptually:

```rust
pub trait ApplicationFactory: Send + Sync {
    fn id(&self) -> &str;

    fn instantiate(
        &self,
        context: ApplicationContext,
    ) -> Result<ApplicationInstance, PluginError>;
}
```

`RhaiApplicationFactory` implements it.

## Two-phase startup

Plugins first register static contributions.

Only after provider/tool resolution and core startup is the chosen app instantiated.

## Scene composers

A plugin can register:

```rhai
host.scene_composers.register(
    "my-sidebar",
    Fn("compose")
);
```

and transform the current scene every frame.

## App replacement test

Create test plugin A:

```text
builtin-like app
```

and test plugin B:

```text
completely unrelated replacement app
```

Start with B selected and assert A is never instantiated.

That guards against the anti-pattern:

```text
load default app
then overwrite it
```

## Scene extension test

Base app:

```text
BODY
```

extension composer wraps it:

```text
SIDEBAR | BODY
```

Disable extension:

```text
BODY
```

No base application state should need to know the extension exists.

## Transaction rollback tests

Plugin registers:

```text
provider
tool
renderer
scene composer
```

then deliberately fails.

After failure none of the four may remain.

---

# 29. Tranche 5 — Reimplement the current Iyon application in Rhai

This is the primary UI migration MR.

Keep the legacy implementation beside it until the entire parity suite passes.

## Port

From:

```text
crates/iyon/src/tui/mod.rs
crates/iyon/src/tui/state.rs
crates/iyon/src/tui/components/
crates/iyon/src/tui/theme.rs
crates/iyon/src/tui/tools/renderers/
```

to:

```text
crates/iyon/rhai/app/
```

The existing app code clearly separates update dispatch, state operations and View derivation, which makes this port relatively direct. 

## Keep native

```text
CoreEventMapper
CoreBridge
AssistantStream
transcript projection pipeline
pipe-table projector
unified-diff parser
native semantic diff values
```

## Rhai owns

```text
Iyon app state orchestration
composer setup
key routing
paste storage policy
working UI composition
approval orchestration
conversation activity control
tool card state
footer
theme
tool renderer selection
exit behavior
```

## Tool renderer migration

The current renderer registry is already a clear feature-extension seam and selects bash/edit/read/write/grep/find/ls with generic fallback. 

Replicate it as a plugin-owned registry.

Rhai renderers receive native render inputs and return native `View`.

### Unified diff

`edit.rhai` calls the native app-support parser and receives `DiffHunk`s.

Then:

```rhai
diff_renderer.render_hunk(hunk)
```

No terminal geometry leaks into script.

## Maintain current public facade

`build_app()` constructs the Rhai-backed implementation.

`IyonState` becomes the opaque native wrapper around that scripting state.

`IyonAction`, `FrontendEvent`, `ToolDraftKey`, and `ToolUpdatePresentation` remain native Rust types.

## Dual parity suite

Every current public app test runs against both implementations.

For deterministic scenarios, assert line-for-line parity.

Especially preserve:

```text
paste placeholder
Ctrl+C semantics
Escape semantics
Shift+Tab
spinner timing
assistant smoothing
tool draft reuse
tool sibling ordering
approval state
tool freezing
composer position
native history movement
Goodbye
```

## Cutover

Only after all parity tests are green:

```text
build_app → Rhai path by default
```

Legacy remains under an internal `legacy-app` test feature for one tranche.

---

# 30. Tranche 6 — Move executable/runtime/auth into `iyon-cli`

Now move process ownership.

## Move

```text
crates/iyon/src/main.rs
crates/iyon/src/auth.rs
```

to `iyon-cli`.

The current binary owns provider detection, auth and interactive startup; none of that is library API, so this move does not need to disturb `iyon`'s public facade.

## Package

```toml
[package]
name = "iyon-cli"

[[bin]]
name = "iyon"
path = "src/main.rs"
```

## Preserve existing CLI

Exactly:

```text
iyon
iyon run

iyon auth login
iyon auth logout
iyon auth status
```

No-subcommand remains equivalent to `run`.

Preserve current environment behavior:

```text
IYON_PROVIDER

openrouter
codex / openai / openai-codex
mock

OPENROUTER_API_KEY
IYON_MODEL
```

Preserve the current default OpenRouter model and Codex model from the existing startup path.

Preserve current fallback-to-Mock semantics where they occur today.

## Replace concrete provider switch with registry resolution

CLI startup becomes:

```text
load provider contributions
      ↓
detect configured provider/model
      ↓
ProviderRegistry::create
      ↓
obtain capabilities
      ↓
build ToolRegistry
      ↓
IyonCoreBuilder
      ↓
spawn
      ↓
instantiate selected application
```

## Add plugin configuration

```text
plugin paths
enabled
disabled
app entry
```

## Tests

CLI snapshots/integration tests:

```text
--help
run --help
auth --help
auth login --help
auth logout
auth status
unknown command
bad config
missing provider
default invocation
```

Also verify package installation produces binary named `iyon`, not `iyon-cli`.

## Completion criteria

`crates/iyon` is library/reference-app only.

`crates/iyon-cli` owns the executable.

---

# 31. Tranche 7 — Dogfood scripted providers and tools, remove legacy privileged paths

This completes the architectural promise.

## Providers

Port protocol semantics for:

```text
Mock
OpenRouter
OpenAI Codex
```

into bundled Rhai provider packages.

Default `iyon-cli` uses these Rhai provider factories.

The existing native public provider structs remain available to Rust library consumers.

## Provider equivalence fixtures

For each provider create fixture-driven tests where both implementations consume identical provider payloads.

Compare:

```text
request URL
method
headers excluding nondeterministic auth where appropriate
request JSON
SSE event interpretation
ModelStreamEvent ordering
tool-call argument reconstruction
Usage
StopReason
ModelErrorKind
```

This prevents script/native implementations from silently diverging.

## Built-in tools

Port:

```text
read
write
edit
bash
grep
find
ls
```

into bundled Rhai registrations using host operations.

Execution and renderer live in the same plugin package.

For example:

```text
tools/read/
  plugin.rhai
  renderer.rhai
```

or one file where appropriate.

## Preserve host primitives

Rust still owns the dangerous/concurrent mechanisms:

```text
workspace path policy
process spawning
file mutation serialization
atomic writes
cancellation
output truncation
stream/channel management
```

Scripts describe operation semantics and result transformation.

## Tool equivalence suite

Run existing native implementation and Rhai implementation against temporary workspace fixtures.

Compare:

```text
ToolResult.content
ToolResult.details
is_error
terminate
updates
filesystem side effects
approval policy
ModelToolSpec
```

## Remove legacy application implementation

Once Rhai path has been the only tested default through the previous tranche:

- delete Rust UI state implementation,
- delete compiled-in renderer registry,
- delete duplicate application orchestration,
- keep only intentional native app-support algorithms.

No `legacy-app` feature remains.

---

# 32. Tranche 8 — Hardening and release-quality closure

This is the final large merge request rather than a cleanup grab bag.

## Stress matrix

At minimum:

```text
1 / 10 / 50 / 100 concurrent provider streams
0 / 10 / 50 / 100 tool operations
rapid cancellation
nested subagent-style activity
slow plugin callbacks
slow network
transport failure
plugin callback failure
plugin operation quota exhaustion
```

## Scheduler assertions

Track:

```text
logical_tasks
pending_operations
callbacks_queued
callbacks_running
max_callbacks_running
callback_latency
event_queue_depth
```

Assert no O(logical-tasks) thread growth.

## Plugin fault isolation

Tests for:

```text
syntax failure
activation failure
callback exception
operation-limit failure
cancelled callback
bad return type
invalid View construction
invalid Projection
invalid StreamSnapshot
duplicate registration
replacement of missing registration
expired scoped borrow
```

## Packaging

Verify embedded `builtin:` scripts are present in:

```text
cargo test
cargo build --release
cargo install --path crates/iyon-cli
```

and do not depend on current working directory.

## Documentation

Generate paired examples:

```text
Rust                       Rhai
----                       ----
View                       view
History                    history
Tool                       tool
Provider                    provider
Theme                      theme
...
```

Generated API coverage document must report:

```text
Rust public API entries: N
Rhai-covered entries:     N
Missing:                   0
Stale bindings:            0
```

## Final deletion gate

Search the repository for:

```text
wasmtime
wit-bindgen
WIT architecture references
legacy app
rhai-unbound-baseline
direct engine.register_fn outside registrar
```

Expected: none except historical documentation explicitly marked as such.

---

# 33. Required test topology after completion

The final CI should have four distinct layers:

```text
1. Native library correctness
   cargo test -p iyon-api
   cargo test -p iyon-core
   cargo test -p iyon-tui

2. Binding correctness
   cargo test -p iyon-plugins --test api_coverage
   cargo test -p iyon-plugins --test binding_smoke
   cargo test -p iyon-plugins --test generic_adapters

3. Product parity
   cargo test -p iyon
   public_app
   provider equivalence
   tool equivalence

4. Architecture/integration
   cargo test -p iyon-plugins --test scheduler_scaling
   cargo test -p iyon-cli
   nightly rustdoc API check
```

The important property is that a developer cannot add:

```rust
pub fn new_feature(...)
```

to `iyon-tui` and get a green PR without simultaneously deciding what:

```text
new_feature
```

means in Rhai.

---

# 34. Explicit non-goals for this refactor

Do not allow the orchestration agent to expand scope into:

- plugin marketplace,
- remote plugin installation,
- semantic-version dependency resolution,
- hot reload,
- WASM compatibility,
- JS/TS bindings,
- Taffy/layout replacement,
- `iyon-components`,
- terminal backend replacement,
- History redesign,
- provider marketplace/catalog service,
- strong hostile-code sandboxing.

The architecture should leave room for those without implementing them.

---

# 35. Definition of complete

The full project is done only when **all** of these are simultaneously true:

1. `iyon-api`, `iyon-core`, and `iyon-tui` compile and remain useful without Rhai.
2. `iyon-api` has a public provider registry.
3. `iyon-core` has a public `Tool` extension contract and public ToolRegistry.
4. Core accepts injected tools rather than hardcoding them internally.
5. `iyon-plugins` is the Rhai host and owns no application-specific Iyon UI semantics.
6. Every reachable public Rust API in the four supplied inventories has a machine-checked Rhai mapping.
7. Future public API drift fails CI automatically.
8. Rhai async activities use event-driven Tokio operations, not one blocked thread per logical task.
9. `Scene` remains a generic native `iyon-tui` concept.
10. Plugin registries remain separate from Scene composition.
11. A plugin may extend the selected application by composing the Scene.
12. A different application may be selected without instantiating the built-in Iyon application.
13. The default Iyon application is Rhai-backed.
14. Its UI and interaction behavior passes the existing public `iyon-tui::testing` suite.
15. Assistant smoothing, semantic streaming, History and native scrollback remain behaviorally unchanged.
16. Tool draft identity and the full tool lifecycle remain unchanged.
17. Default built-in tool execution and rendering pass through the same extension infrastructure exposed to third parties.
18. Default provider implementations pass through the same provider infrastructure exposed to third parties.
19. `iyon-cli` owns process/bootstrap/auth/config/discovery.
20. The executable remains named `iyon`.
21. Existing `iyon::tui` Rust APIs remain available.
22. No permanent “Rhai coverage exceptions” exist.
23. No privileged application-only internal interface exists that the bundled Iyon app relies upon.

The end-state architecture is therefore:

```text
                    iyon-cli
                       │
              process / config / auth
                       │
                       ▼
                 iyon-plugins
         Rhai host / scheduler / registries
          /            │             \
         /             │              \
iyon-api          iyon-core          iyon-tui
providers          agent/tools       semantic TUI
    \                │                 /
     \               │                /
      └────────── builtin:iyon ───────┘
                 Rhai application

                     OR

      └──────── third-party app ──────┘
              exact same host surface
```

That is the architecture I would hand directly to the orchestration agent. The key is that **the migration does not weaken the native architecture in order to support scripting**: it first turns the hidden native extension seams into proper public Rust APIs, then makes Rhai a complete, mechanically verified projection of those APIs, and finally moves the product itself onto that projection.

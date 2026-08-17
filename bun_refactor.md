**Approved as implemented** (2026-08-17, `main` / former `bun-refactor`). Remaining leftovers are intentional and listed in `deferred.md`; they are not current blockers.

Yes. Below is the self-contained implementation handoff I would give the new orchestration agent.

The plan is intentionally decisive: tranches are ordered, boundaries are fixed, and the implementation agent should not reopen architectural choices unless a concrete technical blocker disproves an assumption.

---

# Iyon architecture refactor — implementation handoff

## 0. Mission

Refactor Iyon from the current Rust-owned application into a **Bun-owned TypeScript application and extension platform backed by native Rust libraries**.

The governing architecture is:

> **Bun owns control flow, product behavior, and extensibility. Rust owns correctness-critical native state and the high-performance terminal engine.**

The resulting product must make the bundled Iyon app, agent, tools, and providers use exactly the same extension mechanisms available to third-party packages.

A third-party extension is arbitrary TypeScript running in the normal Bun environment. It is not restricted to being a “tool plugin”, “provider plugin”, etc. Tool/provider/app/agent are **well-known contribution types** that allow the built-in Iyon application to integrate extensions into menus, model selection, tool selection, rendering, and similar product surfaces.

Extensions must additionally be able to use lower-level APIs, hooks, scene composition/replacement, npm dependencies, filesystem/process/network APIs, and other normal Bun facilities.

---

# 1. Current baseline

Base the migration on the current `main`. At the time of this plan, HEAD is `0dc0e4b827da0a485ea2e969e125f5db32756e89`.

The Cargo workspace currently contains:

```text
crates/
  iyon/
  iyon-api/
  iyon-core/
  iyon-tui/
```



The current root architecture document describes an older architecture and must be replaced before implementation begins; it should not be incrementally patched into the new design. 

Existing public API inventories are valuable migration contracts:

```text
iyon-api.md
iyon-core.md
iyon-tui.md
iyon.md
```

For example, `iyon-core.md` explicitly inventories reachable public commands, events, IDs, session types, handles, tool APIs, and methods.  `iyon-api.md` similarly inventories the model protocol as well as concrete provider types that are currently public. 

The existing Rust application also has important public app tests covering composer behavior, paste handling, spinner timing, streaming progression, tools, history positioning, and shutdown. Treat those as a behavioral oracle during the migration. 

The current binary's startup/provider/auth behavior is documented in `iyon.md`, including no-subcommand=run, auth commands, provider fallback, environment variables, and model selection. Preserve those semantics unless the new extension architecture explicitly generalizes them. 

---

# 2. Final repository shape

Target:

```text
.
├── Cargo.toml
├── Cargo.lock
├── package.json
├── bun.lock
├── tsconfig.json
├── ARCHITECTURE.md
│
├── crates/
│   ├── iyon-api/
│   ├── iyon-core/
│   ├── iyon-tui/
│   ├── iyon-native/
│   └── iyon/                  # temporary Rust compatibility crate
│
├── packages/
│   ├── iyon-runtime/
│   ├── iyon-sdk/
│   ├── iyon-plugins/
│   └── iyon-cli/
│
├── plugins/
│   ├── app/
│   │   └── iyon/
│   ├── agents/
│   │   └── iyon/
│   ├── providers/
│   │   ├── mock/
│   │   ├── openrouter/
│   │   └── openai-codex/
│   └── tools/
│       ├── bash/
│       ├── read/
│       ├── write/
│       ├── edit/
│       ├── grep/
│       ├── find/
│       └── ls/
│
└── tools/
    └── api-surface/
```

Bun supports workspaces, so `packages/*` and the bundled plugin directories should be ordinary Bun workspace packages. 

The final executable is generated from the TS CLI using Bun's standalone executable support. Bun supports embedding `.node` N-API addons into compiled executables, which is the deployment model for `iyon-native`. 

---

# 3. Hard dependency boundaries

The Rust graph must be:

```text
iyon-api
   ▲
   │
iyon-core

iyon-tui          # independent of core/api

     ▲      ▲      ▲
     └──────┼──────┘
         iyon-native
```

Specifically:

```text
iyon-api
  no Iyon dependencies

iyon-core
  → iyon-api

iyon-tui
  no iyon-core
  no iyon-api

iyon-native
  → iyon-api
  → iyon-core
  → iyon-tui
```

`iyon-native` is the bridge, **not another application layer**.

Product behavior must never accumulate there.

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

---

# 4. Runtime ownership

Bun owns the process.

Do not retain a Rust `main()` that launches Bun as a subprocess.

The process should conceptually be:

```text
Bun
 │
 ├── CLI
 ├── plugin loader
 ├── default app
 ├── default agent
 ├── providers
 ├── tools
 ├── npm ecosystem
 │
 ▼
iyon-native.node
 │
 ├── iyon-api
 ├── iyon-core
 └── iyon-tui
```

This is important because it eliminates unnecessary bidirectional runtime embedding.

Prefer:

```text
TypeScript
    ↓
native Rust operation
    ↓
Promise
    ↓
TypeScript continuation
```

Avoid designs where possible that require:

```text
Rust background task
    ↓
arbitrary JS callback
    ↓
Promise
    ↓
Rust resumes
```

`napi-rs` supports Rust async functions becoming JS Promises, including Tokio-backed operations. 

Cancellation must nevertheless be explicit: dropping a JS Promise does not automatically cancel a running Rust operation, so native long-lived operations must own cancellation handles/tokens and the TS façade must bridge `AbortSignal` or explicit cancellation onto them. 

---

# 5. Extension-facing modules

Canonical extension imports should remain:

```ts
import ... from "iyon:api";
import ... from "iyon:core";
import ... from "iyon:tui";
import ... from "iyon:plugins";
```

Implement these as Bun virtual/runtime modules.

Bun runtime plugins can intercept module resolution/loading with `onResolve` and `onLoad`, including custom namespaces, so install the Iyon resolver before loading any extension modules. 

`@iyon/sdk` is the development/type package.

It contains generated ambient declarations such as:

```ts
declare module "iyon:api" {
  // generated/public types
}

declare module "iyon:core" {
  // generated/public types
}

declare module "iyon:tui" {
  // generated/public types
}

declare module "iyon:plugins" {
  // TS extension API
}
```

A plugin author can therefore use:

```json
{
  "devDependencies": {
    "@iyon/sdk": "^..."
  }
}
```

while runtime resolution remains controlled by the Iyon host.

Do not require plugin authors to install a native addon.

---

# 6. Fundamental extension model

There are three different concepts:

```text
package
    distribution unit

extension
    arbitrary executable TypeScript module

contribution
    a well-known registration made by an extension
```

Do not use `ToolPlugin`, `ProviderPlugin`, etc. as the fundamental runtime abstraction.

An extension can do all of these simultaneously:

```ts
export default async function activate(iyon: ExtensionAPI) {
  iyon.tools.register(...);
  iyon.providers.register(...);
  iyon.commands.register(...);
  iyon.scene.compose(...);
  iyon.on("sessionStart", ...);
}
```

It is still just **one extension**.

---

# 7. Package manifest

Use:

```json
{
  "name": "@example/iyon-github",
  "version": "1.0.0",

  "keywords": [
    "iyon-package"
  ],

  "dependencies": {
    "@octokit/rest": "^..."
  },

  "devDependencies": {
    "@iyon/sdk": "^..."
  },

  "iyon": {
    "extensions": [
      {
        "id": "main",
        "entry": "./src/index.ts"
      }
    ],

    "compatibility": {
      "iyon": "^0.1.0"
    },

    "catalog": {
      "tools": [
        {
          "id": "github",
          "name": "GitHub"
        }
      ]
    }
  }
}
```

Allow shorthand:

```json
{
  "iyon": {
    "extensions": [
      "./src/index.ts"
    ]
  }
}
```

Rules:

```text
"iyon-package"
    discoverability marker

"iyon.extensions"
    authoritative executable entrypoints

"iyon.catalog"
    static advisory metadata for iyon.dev

runtime registrations
    actual truth about what the extension contributes
```

`catalog` never restricts what the extension may do.

A package may statically advertise one tool and dynamically register five tools, a command, and a Scene transformer.

---

# 8. Contribution registries

`iyon:plugins` should expose at least:

```ts
interface ExtensionAPI {
  readonly tools: ToolRegistry;
  readonly providers: ProviderRegistry;
  readonly agents: AgentRegistry;
  readonly apps: AppRegistry;

  readonly commands: CommandRegistry;
  readonly shortcuts: ShortcutRegistry;

  readonly scene: SceneExtensions;

  on<E extends keyof ExtensionEvents>(
    event: E,
    handler: ExtensionHandler<E>,
  ): Disposable;
}
```

Registration is not a permission boundary.

## Replacement semantics

Duplicate IDs should fail by default.

Replacement must be explicit:

```ts
iyon.tools.register(tool, {
  replace: true,
});
```

Internally use layered registrations:

```text
builtin tool "read"
    ↓
user override
    ↓
project override
```

When the upper registration unloads, the previous registration becomes active again.

This makes hot reload/unload and plugin failure recovery deterministic.

Every contribution must carry source metadata:

```text
package id
extension id
registration id
generation
scope
```

---

# 9. Activation must be transactional

Loading an extension:

```text
discover package
   ↓
parse manifest
   ↓
validate compatibility
   ↓
resolve extension entry
   ↓
begin contribution transaction
   ↓
import module
   ↓
activate(extensionAPI)
   ↓
validate registrations
   ↓
commit
```

If import or activation fails:

```text
rollback every registration
dispose resources
report package/extension identity
continue loading unrelated packages
```

Never leave half-registered extensions.

Registration calls return `Disposable`s.

An activation function may itself return a `Disposable`.

Package unload invokes disposables in reverse activation order.

---

# 10. Apps, Scene extensions, and app selection

These remain three separate concepts:

### App registration

```ts
iyon.apps.register({
  id: "iyon",

  create(ctx) {
    return new IyonApp(ctx);
  },
});
```

### App selection

Config/CLI selects the base application:

```text
selected app = iyon
```

A replacement app starts from its own app.

It must **not** instantiate the built-in Iyon app and then erase it.

### Scene extension

After the selected app produces its Scene, extensions may transform it:

```ts
iyon.scene.compose((scene, ctx) => {
  return scene.withBody(
    View.vertical([
      scene.body,
      myWidget(),
    ]),
  );
});
```

An extension may also replace the Scene:

```ts
iyon.scene.replace((_scene, ctx) => {
  return createCompletelyDifferentScene(ctx);
});
```

Therefore:

```text
selected app
    ↓
base Scene
    ↓
scene extension chain
    ↓
final Scene
    ↓
iyon-tui
```

This is the required “do anything” escape hatch.

---

# 11. `iyon-api` final role

`iyon-api` remains Rust primarily because it is already a useful provider-neutral protocol and native core contract.

Final long-term responsibility:

```text
ModelApi protocol
ModelRequest
ModelMessage
ContentBlock
ModelToolSpec
ModelStreamEvent
ModelParams
ModelMetadata
Usage
StopReason
ModelError
ModelErrorKind
ReasoningLevel
CacheRetention
```

Concrete providers do **not** belong in the long-term API crate.

Current concrete provider types are public today. 

Therefore migrate them in two stages:

```text
product runtime
    immediately stops using Rust provider implementations

Rust compatibility API
    keeps deprecated provider types temporarily

future intentional breaking release
    removes legacy concrete providers from iyon-api
```

Do not force removal during the first architecture migration.

Provider-specific reasoning capability logic likewise moves to provider registrations.

`ReasoningLevel` remains protocol.

Provider-specific:

```text
supported reasoning levels
model defaults
model discovery
authentication
```

belongs in TS provider contributions.

---

# 12. Provider design

Example:

```ts
export default function activate(iyon: ExtensionAPI) {
  iyon.providers.register({
    id: "openrouter",

    async create(config) {
      return new OpenRouterProvider(config);
    },

    capabilities(model) {
      return {
        reasoning: [...],
      };
    },

    auth: {
      async login(ctx) {},
      async logout(ctx) {},
      async status(ctx) {},
    },
  });
}
```

Provider implementation:

```ts
class OpenRouterProvider implements ModelApi {
  async *stream(
    request: ModelRequest,
  ): AsyncIterable<ModelStreamEvent> {
    const response = await fetch(...);

    // provider protocol parsing
    // yield normalized iyon:api events
  }
}
```

Do not build an Iyon HTTP client abstraction merely because providers need HTTP.

Providers should normally use Bun/Web/npm networking APIs directly.

---

# 13. `iyon-core` final role

Do **not** define `iyon-core` as “the Iyon agent”.

Define it as:

> **A provider-agnostic native execution kernel that owns canonical agent/session state and lifecycle invariants.**

Keep Rust where it gives a coherent native correctness/concurrency boundary, not simply because code happens to already be Rust.

Core owns:

```text
IDs
canonical transcript/session state
model-stream normalization
tool-call stream assembly
tool lifecycle state
approval state
cancellation
bounded event channels
core events
steering/follow-up queues
validated state transitions
optional resource ceilings
```

Core does **not** own:

```text
default system prompt
provider selection UX
provider implementation
tool implementation
tool rendering
default agent continuation policy
request composition policy
reasoning cycling policy
context pruning/compaction policy
TUI
```

---

# 14. Native core primitives

Create a low-level public kernel API.

Conceptually:

```rust
pub struct KernelSession { ... }

pub struct ModelTurn { ... }

pub struct ToolExecution { ... }

pub struct CoreEventReceiver { ... }
```

The exact Rust names can follow existing naming conventions, but the semantic boundaries below are mandatory.

---

## Model turn primitive

TS does provider execution.

Rust consumes normalized provider events.

Conceptually:

```ts
const turn = session.beginModelTurn({
  request,
});

try {
  for await (const event of provider.stream(request)) {
    turn.push(event);
  }

  const result = await turn.finish();
} catch (error) {
  await turn.fail(error);
}
```

Rust owns:

```text
assistant message ID
Start/Delta/End normalization
text/thinking accumulation
tool draft identity
{message_id, content_index}
late provider tool IDs
tool argument assembly
usage
stop reason
partial reply preservation
core event emission
cancellation
```

The current native `run_model_turn` implementation already performs much of this normalization and is the starting point for extraction, not something to rewrite in TS.

Provide both:

```ts
turn.push(event);
turn.pushMany(events);
```

so future batching is possible without public API change.

---

## Tool execution primitive

One TS tool combines execution and presentation:

```ts
defineTool({
  name: "read",
  description: "...",
  inputSchema: ...,

  async execute(ctx, args) {
    ...
  },

  renderCall(call) {
    ...
  },

  renderResult(result) {
    ...
  },
});
```

Core does not call arbitrary JS itself unless there is no cleaner path.

Instead TS drives the native lifecycle:

```text
model produced tool call
        ↓
TS agent selects registered Tool
        ↓
native ToolExecution created
        ↓
TS before-tool hooks
        ↓
native transition / approval
        ↓
TS tool.execute()
        ↓
native updates
        ↓
TS after-tool hooks
        ↓
native finish/fail
        ↓
canonical transcript result
```

Native state machine remains:

```text
Preparing
    ↓
Prepared
    ↓
Running
    ↓
PendingApproval
    ↓
Running
    ↓
Finished / Failed / Cancelled
```

Do not fake execution or success to simplify UI.

---

# 15. Approval separation

Native core owns:

```text
ApprovalId
pending approval state
waiting
resolution
cancellation
events
validated transitions
```

Policy lives outside core.

For example, the current special treatment of `sudo` belongs in the bundled bash tool/default extension policy rather than a generic core function.

This allows third-party tools to define comparable approval behavior.

---

# 16. Canonical transcript vs model context

These must be different concepts.

Rust owns canonical session history.

TS agent policy chooses what becomes the next `ModelRequest`.

Expose mechanical native helpers such as:

```ts
session.entries();
session.messages();

session.appendMessage(...);
session.appendEntry(...);

session.toModelMessages(entries);
```

Then an agent can do:

```ts
const canonical = session.messages();

const selected =
  await transformContext(canonical);

const request: ModelRequest = {
  systemPrompt,
  messages: session.toModelMessages(selected),
  tools: activeTools,
  params,
  metadata,
};
```

Compaction/RAG/filtering must not require destructive mutation of canonical history.

---

# 17. Extensible session entries

Add native persistent extension entries.

Conceptually:

```rust
pub enum SessionEntry {
    Message(AgentMessage),

    Custom {
        namespace: String,
        data: serde_json::Value,
    },
}
```

This allows extensions to persist:

```text
plans
checkpoints
agent metadata
memory data
handoffs
extension state
compaction markers
```

without pretending those are model/user messages.

Namespaces should be package-style identifiers to avoid collisions.

---

# 18. Steering and follow-up

Do not retain one overloaded new-input operation as the long-term public API.

Expose:

```ts
agent.prompt(message);
agent.steer(message);
agent.followUp(message);
agent.abort();
```

Semantics:

```text
prompt
    starts a new agent run

steer
    enters the current run at its next safe turn boundary

followUp
    queues work after the active run settles

abort
    cancels current work while preserving the defined partial-state semantics
```

Keep queues native for ordering/cancellation correctness.

The existing `SubmitTurn` behavior remains as a compatibility wrapper for Rust callers.

---

# 19. Default agent becomes TS

Bundled:

```text
plugins/agents/iyon/
```

The default agent owns:

```text
request composition
system prompt
tool selection
context transformation
reasoning selection
stop-reason continuation
tool-round policy
steering policy
follow-up policy
compaction policy
```

Conceptually:

```ts
class IyonAgent implements Agent {
  async run(ctx: AgentContext): Promise<void> {
    while (!ctx.signal.aborted) {
      const request =
        await this.buildRequest(ctx);

      const turn =
        await runProviderTurn(ctx, request);

      if (!this.shouldContinue(turn, ctx)) {
        return;
      }

      await executeRequestedTools(ctx, turn.toolCalls);
    }
  }
}
```

The current limit of 16 requested tools per model turn becomes default agent policy.

If core retains a much larger hard ceiling to prevent runaway resource usage, document that explicitly as a **host safety ceiling**, not product behavior.

---

# 20. High-level and low-level core APIs

Provide two TS power levels.

## High level

For ordinary extensions:

```ts
AgentSession

.prompt()
.steer()
.followUp()
.abort()

.setModel()
.setReasoning()
.setActiveTools()

.events()
```

## Low level

For arbitrary agents:

```ts
KernelSession

.beginModelTurn()
.prepareToolExecution()
.appendMessage()
.appendEntry()
.events()
...
```

The bundled agent must itself be implementable using only the low-level public API.

That is the test that there is no privileged internal agent API.

---

# 21. `iyon-tui` remains the performance center

Do not rewrite the terminal engine in TS.

Rust retains:

```text
terminal I/O
native scrollback
layout
wrapping
paint
History
projection
streaming mechanics
TextInput state
Diff rendering internals
incremental source mapping
Smooth
terminal sizing/input
headless terminal simulation
```

Its current dependency footprint confirms it is already a self-contained terminal framework independent of core/provider concerns. 

---

# 22. TUI TS binding strategy

Do **not** naïvely generate one N-API call for every fluent `View` builder operation.

Separate Rust APIs into:

```text
pure semantic values
    → lazy TS values

stateful native objects
    → native N-API handles

async/runtime operations
    → native Promise operations
```

---

## Lazy semantic values

Examples:

```text
View
TextContent
style values
Insets
BorderSpec
DiffHunk
DiffLine
other immutable semantic builders
```

Public API stays coherent with Rust:

Rust:

```rust
View::text("hello")
    .bold()
    .padding(1)
```

TS:

```ts
View.text("hello")
  .bold()
  .padding(1)
```

No native crossings are required while constructing the value.

Internally:

```text
TS View
    ↓
private semantic builder IR
    ↓
one materialization/lowering operation
    ↓
Rust View
```

Do not expose the bridge IR publicly.

---

# 23. Lazy values must preserve Rust semantics

If a Rust operation behaves as a value builder:

```rust
let a = View::text("foo");
let b = a.bold();
```

the TS API should behave similarly:

```ts
const a = View.text("foo");
const b = a.bold();
```

Do not silently mutate `a`.

Builder operations should return logically new values.

Naming must stay deterministic.

Natural identical names remain identical:

```text
vertical
horizontal
bold
padding
border
```

Rust snake_case maps deterministically to TS camelCase:

```text
set_text            → setText
active_tool_names   → activeToolNames
```

The mapping belongs in generated binding metadata.

---

# 24. Stateful TUI objects

Use native handles for:

```text
History
TextInput
StreamPane
TextStream
native Component state
AppHandle-like runtime state
terminal runtime
```

Example:

```ts
const input = new TextInput({
  multiline: true,
});
```

creates a native Rust object.

When a lazy `View` refers to a native component, its private IR stores a handle reference:

```text
View tree
 ├── lazy text
 ├── lazy container
 └── native component handle
```

Materialization resolves the handle without duplicating component state.

---

# 25. Scene model

Keep the generic core:

```text
Scene {
  history?: History
  body: View
}
```

The selected app produces a Scene.

Scene modifiers operate on this semantic object.

The native TUI remains responsible for final layout/history/terminal behavior.

---

# 26. Preserve native scrollback exactly

Do not regress the existing history design.

Promoted rows must still become native terminal scrollback by:

```text
paint promoted rows at top
cursor at bottom
emit ordinary full-screen CRLFs
terminal scrolls rows into native scrollback
mirror model with ScrollRegionUp
validate differential state
```

Never replace that with:

```text
DEC scrolling region tricks
private fake scrollback
model-only row mutation
```

This remains a frozen behavioral invariant.

---

# 27. TUI callback/trait bindings

The API parity requirement eventually includes public traits such as renderers/projectors/streaming abstractions.

Use an explicit binding strategy.

Examples:

```text
Renderer
Projector
TextVisitor
TextRewriter
StreamingSource
Component
```

TS-facing interfaces should represent those capabilities naturally.

When a native pipeline genuinely needs to invoke a TS implementation, create a dedicated N-API adapter that marshals execution onto the Bun/JS thread.

Do not allow arbitrary Tokio workers to directly manipulate JS values.

These adapters should be the exception, not the default architecture.

---

# 28. Public API parity and drift prevention

This is mandatory infrastructure, not documentation work.

Build:

```text
tools/api-surface/
```

using:

```text
syn
cargo_metadata
```

The scanner determines the **externally reachable Rust API**, not every item containing `pub`.

Handle:

```text
public/private modules
pub use
pub use through private modules
nested re-exports
glob re-exports
aliases
struct fields
enum variants
variant fields
free functions
consts/statics
traits
associated items
inherent methods
trait implementation projections
cfg
features
```

Preserve every reachable path/alias.

---

# 29. Canonical binding schema

Pipeline:

```text
Rust public API
      ↓
API surface scanner
      ↓
canonical binding schema
      ├── generated TS declarations
      ├── binding implementation metadata
      ├── generated coverage report
      └── parity tests
```

Every item receives a strategy, for example:

```text
MirrorValue
LazyValue
NativeHandle
NativeSync
NativeAsync
TraitAdapter
TsFacade
CompatibilityProjection
```

There is **no permanent Ignore/Unsupported category**.

If an item cannot be represented literally, it receives a deliberate semantic TS projection.

CI requires:

```text
reachable Rust APIs: N
mapped TS APIs:      N

missing:             0
stale:               0
```

A nightly rustdoc-JSON cross-check may verify the source scanner, but the normal CI path must work on stable Rust.

---

# 30. Compatibility policy

Do not casually delete currently public Rust APIs.

## `iyon-api`

Keep protocol APIs.

Concrete providers become deprecated compatibility implementations once TS providers replace them in the product.

## `iyon-core`

Keep existing IDs, tool types, commands/events, and handles where feasible.

Build the existing `IyonCore` behavior as a compatibility/reference driver over the new kernel primitives.

New product code does not need to use it.

## `iyon-tui`

Preserve public Rust API.

The TS binding is additive.

## `crates/iyon`

Do not delete immediately.

Once the TS application becomes the actual product binary:

```text
crates/iyon
    deprecated Rust compatibility application/library
```

It receives no new product features.

Remove the Rust executable target once the Bun executable has full parity, but retain the library for the compatibility window.

Remove the compatibility crate only in a later intentional breaking release.

---

# 31. Tranche rules

Each tranche below must be independently mergeable.

Every tranche:

```text
starts from green main
has narrowly defined scope
adds its own verification
leaves both Rust and new TS paths buildable
does not depend on unfinished code in the next tranche
```

Do not begin large migrations before the prerequisite tranche's exit criteria pass.

---

# Tranche 0 — Replace the architecture contract and freeze migration invariants

## Goal

Make the repository describe the architecture being built before adding infrastructure.

## Work

Rewrite `ARCHITECTURE.md` around:

```text
Bun process ownership
Rust native libraries
N-API membrane
arbitrary TS extensions
package/extension/contribution distinction
apps/tools/providers/agents as registrations
Scene replacement
core kernel
TUI binding model
API parity
```

Remove descriptions of architecture that is no longer planned.

Document the final dependency graph.

Add a migration document containing:

```text
current location
target location
migration tranche
compatibility disposition
```

for all major current modules.

Explicitly document frozen behavior:

```text
native scrollback algorithm
semantic View before geometry
Theme not geometry
tool lifecycle
streaming model Start/Delta/End
draft key {message_id, content_index}
composer behavior
approval semantics
stream smoothing
Goodbye behavior
```

Mark the four existing public API inventories as migration baselines.

## Tests

No runtime behavior changes.

Run existing Rust checks.

## Exit criteria

The architecture document alone is sufficient for a fresh engineer to answer:

```text
who owns the process?
where do plugins run?
what does core own?
what does TUI own?
how does TS talk to Rust?
can an extension replace the Scene?
what distinguishes app/tool/provider registrations?
```

No code migration before this is merged.

---

# Tranche 1 — Bun + N-API viability slice

## Goal

Prove the complete process/runtime/distribution model before binding real APIs.

## Add

```text
package.json
bun.lock
tsconfig.json

packages/iyon-runtime/
packages/iyon-cli/

crates/iyon-native/
```

Add `napi-rs` to `iyon-native`.

The root becomes a Bun workspace.

## Native smoke APIs

Implement deliberately small probes:

```text
nativeVersion()

echoJson()

asyncSleep()
    Rust async/Tokio → Promise

CancellationProbe
    explicit cancellation token

NativeCounter
    native class/handle/finalizer

EventQueueProbe
    Tokio channel → JS nextEvent Promise
```

Also expose one trivial `iyon-tui` native operation.

## Virtual module loader

Install a Bun runtime plugin before application imports.

Prove:

```ts
import { ... } from "iyon:api";
import { ... } from "iyon:core";
import { ... } from "iyon:tui";
```

work.

Bun supports runtime import interception through its plugin API. 

## Standalone build

Build:

```text
iyon-smoke
```

with `bun build --compile`.

Embed `iyon-native.node`.

Bun documents embedded N-API addons in standalone executables. 

## Critical tests

Test:

```text
Rust async → Promise
Promise success
Promise failure
explicit cancellation
native handle finalization
JSON conversion
large String transfer
Buffer transfer
100 concurrent Rust futures
Tokio channel receiver
clean shutdown
addon embedded in standalone executable
```

Run on supported target CI platforms.

Bun currently describes its Node-API implementation as incomplete rather than 100%, so do not proceed merely because a trivial addon loads; prove the exact N-API operations Iyon needs. 

## Exit criteria

The final process topology is proven:

```text
compiled Bun executable
        ↓
embedded iyon-native.node
        ↓
Tokio/native Rust operation
```

No fallback child-process architecture.

---

# Tranche 2 — Public API scanner and binding contract

## Goal

Prevent the migration from producing an ad-hoc TS API that slowly diverges from Rust.

## Build

```text
tools/api-surface/
packages/iyon-sdk/
```

Scanner inputs initially:

```text
iyon-api
iyon-core
iyon-tui
iyon
```

Generate a canonical machine-readable API manifest.

Generate:

```text
iyon-api declarations
iyon-core declarations
iyon-tui declarations
coverage report
mapping report
```

The SDK package supplies editor/type support for the virtual `iyon:*` modules.

## Add CI

Fail on:

```text
new reachable Rust API without mapping
removed Rust API with stale TS mapping
signature drift
re-export drift
feature/cfg drift
```

Add a separate nightly rustdoc-JSON verification job if useful.

## Exit criteria

Every current public Rust API has a recorded disposition.

Coverage:

```text
missing = 0
stale   = 0
```

This gate remains mandatory for every later tranche.

---

# Tranche 3 — Refactor `iyon-core` into a kernel

## Goal

Create the native primitives needed by arbitrary TS agents while preserving the existing Rust-facing API.

## Model/session work

Introduce:

```text
KernelSession
SessionEntry
SessionSnapshot
native message append/query operations
native ID ownership
```

Separate canonical transcript state from model request construction.

Make custom namespaced extension entries possible.

## Model turn work

Extract current model-stream normalization into a public kernel primitive:

```text
begin
push event(s)
finish
fail/cancel
```

Preserve:

```text
tool draft identity
partial replies
usage
thinking
content ordering
tool-call assembly
event ordering
```

## Tool execution work

Extract a native lifecycle handle/state machine.

Remove product-specific policy from the primitive.

Approval remains native state.

Approval requirement becomes supplied policy/decision.

## Queue work

Introduce explicit native:

```text
steering queue
follow-up queue
```

Do not overload their public semantics.

## Registry work

New kernel construction accepts/injects what it needs.

The new kernel path must **not** call `register_builtin_defaults`.

Current core does hardcode its built-in tool registry today, so this is a required separation. 

## Compatibility

Refactor current `IyonCore` onto the new primitives where practical.

Keep existing public commands/events working.

Keep legacy built-ins available only to the compatibility driver until their eventual removal.

## Exit criteria

A Rust test can manually implement:

```text
session
→ model turn
→ assembled tool call
→ tool lifecycle
→ transcript result
```

without invoking the current hard-coded agent loop.

The current Rust `IyonCore` tests remain green.

---

# Tranche 4 — `iyon:api` and `iyon:core` TypeScript surfaces

## Goal

Expose the new kernel naturally to Bun.

## `iyon:api`

Map all protocol types.

Prefer normal TS discriminated unions for Rust enums where appropriate.

Example:

```ts
type ModelStreamEvent =
  | { type: "started" }
  | { type: "textDelta"; ... }
  | { type: "thinkingDelta"; ... }
  | { type: "toolCallStart"; ... }
  | ...
```

The generated schema remains authoritative.

## `iyon:core`

Expose:

```text
KernelSession
session snapshots
events
model-turn handle
tool-execution handle
approval operations
queue operations
cancellation
```

Wrap:

```text
nextEvent(): Promise<Event | null>
```

into an ergonomic:

```ts
AsyncIterable<CoreEvent>
```

on the TS side.

Bridge `AbortSignal` to explicit native cancellation.

Do not assume rejected/abandoned JS promises cancel native tasks.

## Exit criteria

A completely synthetic TS agent can:

```text
create session
append user message
feed fake model events
observe assistant events
prepare/finish fake tool result
inspect canonical transcript
cancel an active operation
```

without using the default Iyon app.

---

# Tranche 5 — Full `iyon:tui` TypeScript binding

## Goal

Make TS capable of building arbitrary Iyon applications without sacrificing the Rust terminal engine.

## Phase 5A: value types

Implement lazy TS façades for the pure semantic API.

The public API should mirror the Rust builder semantics.

Do not expose bridge IR.

Start with the types necessary to reproduce the default app, then continue until the API scanner reaches 100%.

## Phase 5B: native handles

Bind stateful objects:

```text
History
TextInput
streams
components
terminal runtime
```

## Phase 5C: runtime

Expose a TS-owned app loop:

```ts
const tui = await Tui.open();

while (!done) {
  const event = await tui.nextEvent();

  update(event);

  const scene = view();

  await tui.render(scene);
}
```

The exact ergonomic wrapper may differ, but control flow stays TS-owned.

Rust still performs:

```text
terminal input
layout
wrapping
History
projection
paint
native scrollback
terminal writes
```

## Phase 5D: headless harness

Expose enough of the Rust test backend to test TS applications deterministically:

```text
fixed terminal width/height
key input
paste
clock advancement
native history rows
screen rows
style lookup
```

## Phase 5E: public traits

Add semantic TS adapters for remaining public extension traits until API coverage is complete.

## Exit criteria

A TS-only demo application can reproduce:

```text
composer
history
streaming text
native history promotion
component focus/input
Scene rendering
```

and API scanner coverage for `iyon-tui` is 100%.

---

# Tranche 6 — Extension/package runtime

## Goal

Make arbitrary TypeScript packages loadable using the real final extension API.

## Implement

```text
package discovery
manifest parsing
extension loading
transactional activation
registries
source ownership
replacement/unload
compatibility checks
event hooks
Scene composition
app selection
```

## Load scopes

Use deterministic precedence:

```text
1 bundled packages
2 user-installed packages
3 project-local packages
```

Load order within each scope is deterministic.

Duplicate registration does not silently replace another contribution.

Overrides must request replacement explicitly.

## Package sources

Support as the public model:

```text
npm:
git:
local path
```

Do not make package-source semantics part of extension execution APIs.

Bun already provides normal package/module resolution and npm-style dependency behavior; use that ecosystem rather than inventing a custom dependency declaration format. 

## Security model

Extensions are arbitrary code running with the user's Iyon process privileges.

Do not fake a sandbox by omitting useful APIs.

Any future permissions system is a runtime security policy layered on top, not an artificially crippled extension API.

## Fixture packages

Create integration fixtures:

```text
register one tool
register multiple contribution types
override builtin
activation failure rollback
unload restores previous override
Scene composer
full Scene replacement
async activation
npm dependency import
```

## Exit criteria

Bundled and external fixture extensions travel through the same loader.

There is no `is_builtin` execution shortcut.

---

# Tranche 7 — Migrate bundled providers

## Goal

Make the product stop using Rust concrete providers.

Implement:

```text
plugins/providers/mock/
plugins/providers/openrouter/
plugins/providers/openai-codex/
```

Providers use normal Bun networking/runtime APIs.

Port:

```text
request serialization
stream parsing
error normalization
usage
stop reasons
tool-call streaming
provider capabilities
default model data
auth hooks
```

## Auth

Move provider-specific authentication orchestration into provider contributions.

If native keychain storage remains desirable, expose only a generic native credential-store primitive.

Do not put OpenRouter/OpenAI-specific knowledge into `iyon-native`.

## Tests

Use captured/synthetic provider stream fixtures.

Assert produced `ModelStreamEvent` sequences against the existing Rust implementations.

Test fragmented SSE/JSON boundaries heavily.

## Cutover

Once parity passes, default product startup uses TS providers exclusively.

Rust concrete providers become compatibility-only.

`reqwest` and provider-specific dependencies can eventually disappear from the pure protocol path; currently `iyon-api` directly depends on networking/provider-related crates. 

## Exit criteria

Deleting the TS provider registrations causes the product to have no provider.

There is no hidden native default provider in the main product path.

---

# Tranche 8 — Migrate bundled tools

## Goal

Make built-in tools real extensions.

Create:

```text
plugins/tools/bash
plugins/tools/read
plugins/tools/write
plugins/tools/edit
plugins/tools/grep
plugins/tools/find
plugins/tools/ls
```

Every tool owns both:

```text
execution semantics
presentation semantics
```

Example:

```ts
defineTool({
  name: "edit",

  async execute(ctx, args) {
    ...
  },

  renderCall(call) {
    ...
  },

  renderResult(result) {
    ...
  },
});
```

The default app only understands generic tool lifecycle.

It must contain no:

```text
if toolName === "bash"
if toolName === "read"
if toolName === "edit"
```

presentation logic.

## Workspace helpers

Existing safe path/mutation primitives may remain available as native convenience APIs if useful.

Bundled tools may use them to retain current behavior.

They are not the only filesystem capability available to third-party extensions.

## Approval policy

Move bash/sudo behavior into the bash contribution or product tool policy.

## Generic fallback

Unknown tools must still render generically.

## Override test

A fixture package must be able to replace `read` atomically:

```text
execution
+
renderCall
+
renderResult
```

without changing the app.

## Exit criteria

The product has no main-path dependency on `iyon-core/src/tools/builtin/*`.

Bundled tools are loaded through the same package/extension registration process as third-party tools.

---

# Tranche 9 — Migrate the default agent

## Goal

Make Iyon's agent algorithm a bundled TS extension rather than privileged core behavior.

Create:

```text
plugins/agents/iyon/
```

Port:

```text
request building
default system prompt
context selection
active tool exposure
reasoning selection
continuation
stop-reason decisions
tool execution sequencing
steering
follow-up behavior
tool-call count policy
```

Keep existing behavior first.

Do not introduce parallel tool execution merely because the new architecture makes it possible.

That can be a later feature.

## Agent registration

```ts
iyon.agents.register({
  id: "iyon",

  create(ctx) {
    return new IyonAgent(ctx);
  },
});
```

## Low-level proof

Add a fixture custom agent whose algorithm differs materially from Iyon:

```text
different request composition
subset of tools
custom stop condition
custom context transformation
```

It must use only public `iyon:core`.

## Subagent proof

Add an integration test demonstrating that an agent can create/run another independent agent/session without private APIs.

It does not need full product UX.

## Exit criteria

The built-in Iyon agent does not have an internal Rust capability unavailable to third-party TS agents.

---

# Tranche 10 — Migrate the default application and CLI

## Goal

Make the real shipped Iyon product Bun/TS-owned.

Create:

```text
plugins/app/iyon/
packages/iyon-cli/
```

## Default app

Port:

```text
conversation state
composer
backend event mapping
tool draft presentation
approvals
status/footer
reasoning UI
working spinner
tool cards
streaming assistant display
exit UX
```

Use the native generic projection/streaming/TUI primitives rather than reimplementing rendering machinery in TS.

Keep particularly sensitive streaming behavior native where it corresponds to reusable TUI primitives.

Do not move product-specific behavior into `iyon-native` merely to avoid porting it.

## Required UX parity

Preserve:

```text
MAX_COMPOSER_ROWS = 13

large paste:
  >1000 chars
  OR
  >10 lines

CRLF/CR/tab normalization

Ctrl+C semantics
Escape cancellation
Shift+Tab reasoning action
multiline composer borders

footer:
  provider
  model
  effort
  status

draft tool visibility
draft reuse across late provider IDs
tool lifecycle cards
smoothing/pacing
History bottom positioning
approval UI
frozen results
Goodbye
native scrollback
```

## Streaming parity

Port the existing Rust public app tests to the new TS headless harness before deleting any old implementation.

The existing test suite already checks several of these user-visible invariants. 

Keep both tests running during overlap:

```text
old Rust app oracle
new TS app parity tests
```

## CLI

Move process/bootstrap parsing to TS.

Preserve:

```text
iyon
    → run

iyon run

iyon auth login
iyon auth logout
iyon auth status
```

Preserve current provider-selection/environment fallback behavior until provider registration generalizes it. The existing behavior is documented in `iyon.md`. 

The CLI:

```text
load config
initialize native addon
initialize virtual modules
discover packages
activate extensions
select provider
select agent
select app
run app
cleanup terminal/runtime
```

## Cutover

Once parity is demonstrated:

```text
Bun standalone executable becomes canonical `iyon`
```

Remove the Rust `main.rs` product binary.

Retain `crates/iyon` as a deprecated library compatibility layer.

## Exit criteria

The shipped binary runs:

```text
TS CLI
→ TS plugin runtime
→ TS default app
→ TS default agent
→ TS providers/tools
→ Rust core/TUI
```

No first-party shortcut exists.

---

# Tranche 11 — TUI bridge optimization

## Goal

Optimize the part where native Rust performance actually matters.

Do this **after functional parity**, but treat it as a real architecture tranche rather than indefinite cleanup.

## Benchmark first

Measure:

```text
View construction
N-API crossings
View materialization
layout
projection
terminal rendering
streaming updates
History promotion
```

Workloads:

```text
10 / 100 / 1000 View nodes

10 / 50 / 200 stream updates per second

1k / 10k / 100k frozen history rows

1 / 10 / 50 Scene transformations

repeated TextInput activity
```

## Optimize lazy View materialization

Initial implementation may pass a recursive bridge representation.

Next optimization may compile it into a compact command buffer:

```text
TEXT
STYLE_BOLD
PADDING
TEXT
VERTICAL
...
```

Then perform:

```ts
native.materializeView(buffer, strings, handles);
```

once.

The public API stays:

```ts
View.text("hello")
  .bold()
  .padding(1);
```

No public extension code changes.

## Other possible optimizations

Only after profiling:

```text
string interning
style interning
stable subtree caching
batched native component references
batched model-stream updates
bridge allocation reuse
```

Never trade away native History/scrollback correctness for a synthetic benchmark.

## Exit criteria

The TS app's TUI path is within an explicitly recorded acceptable overhead relative to the Rust baseline, with no user-visible streaming degradation.

---

# Tranche 12 — Hardening, packaging, ecosystem readiness

## Runtime stress

Test:

```text
100 extensions
many simultaneous registrations
override/unload cycles
activation failures
50+ concurrent Rust async operations
agent cancellation storms
provider failure during streaming
tool failure during approval
shutdown with outstanding work
```

## Plugin error isolation

One extension exception must not corrupt registries or terminal state.

Errors include package and extension provenance.

## Distribution

Build/test standalone binaries for supported targets.

Verify embedded native addon loading on each platform.

Bun's standalone executable mechanism bundles the Bun runtime and supports embedded N-API addons, making that the required release path. 

## Catalog

Implement the future iyon.dev index around:

```text
npm search/discovery for "iyon-package"
        ↓
package metadata
        ↓
iyon.catalog
        ↓
apps/tools/providers/agents classification
```

Never execute third-party package JS to build the website catalog.

## Docs

Document:

```text
create extension
package manifest
tool
provider
agent
app
Scene extensions
low-level core
TUI
package install
security model
```

Bundled extensions should be the canonical examples.

---

# 32. Generated binding implementation policy

The API generator must not force every Rust method to become a native N-API export.

For example:

```text
Rust View::bold
    ↓ mapped public API
TS View.bold
    ↓ implemented lazily
```

That still counts as complete binding coverage.

Likewise:

```text
Rust generic App
    ↓
TS semantic App projection
```

can be a deliberate façade rather than attempting to expose a Rust monomorphized generic directly.

The important invariant is:

> Every externally reachable Rust capability has a documented, tested TS path.

Not:

> Every Rust function gets a 1:1 FFI symbol.

---

# 33. Native boundary rules

Use these rules throughout implementation.

### Rule 1

Own data across async boundaries.

Do not hold borrowed N-API values while a Tokio future is suspended.

### Rule 2

Rust panics must never casually unwind through Bun.

Return typed errors.

### Rule 3

Every long-lived async native operation has an explicit owner and cancellation path.

### Rule 4

Never let a Tokio background task outlive native handles/runtime state it references.

### Rule 5

Prefer data/value crossings to callbacks.

### Rule 6

When Rust genuinely calls JS, marshal onto the JS thread correctly.

### Rule 7

Do not serialize native TUI state to JSON merely because JSON is easy.

Use native handles for stateful objects.

### Rule 8

Do not duplicate terminal/layout logic in TS.

---

# 34. What should **not** be built

Do not build:

```text
custom JS engine
embedded Bun runtime inside Rust
Rust HTTP layer just for providers
custom npm ecosystem
private-scrollback TUI
JSON-based duplicate TUI renderer
one-plugin-one-kind restrictions
provider/tool allowlists
mandatory Iyon filesystem/network APIs
hidden builtin tool/provider/app registrations
separate builtin extension loader
special privileged default agent APIs
```

---

# 35. Architectural acceptance tests

At the end, all of these must be true.

### Arbitrary extension

A package can:

```text
import arbitrary npm dependency
read filesystem
spawn a process
make network request
register a tool
register a command
listen to session events
change the Scene
```

from one extension entrypoint.

### Builtin tool parity

Replacing `read` requires no changes to the default app.

### Provider parity

Adding a provider causes the default app's provider/model UI to discover it through registration metadata.

### Agent parity

A third-party agent can implement a completely different loop using the same kernel operations as the default agent.

### App parity

A third-party app can be selected as the base app and does not instantiate the default Iyon app.

### Scene power

An unrelated extension can replace the selected app's Scene without registering a new app.

### Rust API

A pure Rust consumer can continue using `iyon-api`, `iyon-core`, and `iyon-tui` without Bun.

### TypeScript API

Every reachable public API in those crates has a machine-tracked TS path.

### TUI

Native scrollback, streaming smoothness, history promotion and composer placement remain correct.

### Distribution

The actual shipped `iyon` executable contains Bun plus the native addon and requires no separate JS runtime installation.

---

# 36. Migration sequencing summary

The orchestration order is deliberately:

```text
0  architecture contract
        ↓
1  Bun/N-API feasibility
        ↓
2  API parity infrastructure
        ↓
3  Rust kernel refactor
        ↓
4  core/api TS binding
        ↓
5  full TUI TS binding
        ↓
6  extension/package runtime
        ↓
7  providers
        ↓
8  tools
        ↓
9  agent
        ↓
10 app + CLI cutover
        ↓
11 TUI bridge optimization
        ↓
12 hardening/distribution/ecosystem
```

Do **not** migrate the default app early.

It is intentionally near the end because it exercises almost every lower boundary simultaneously.

Likewise, do not prematurely optimize the TUI bridge before the lazy semantic representation and parity tests are stable.

---

# 37. Definition of architectural completion

The refactor is complete when this is literally true:

```text
                         iyon
                Bun standalone executable
                          │
      ┌───────────────────┼────────────────────┐
      │                   │                    │
   TS app              TS agent           extensions
      │                   │                    │
      ├──────────────┬─────┴──────┬─────────────┤
      │              │            │             │
  providers         tools      commands       Scene
      │              │            │             │
      └──────────────┴──────┬─────┴─────────────┘
                            │
                     iyon:* public APIs
                            │
                     iyon-native.node
                            │
          ┌─────────────────┼──────────────────┐
          │                 │                  │
      iyon-api          iyon-core          iyon-tui
       protocol          kernel          terminal engine
```

with this rule holding universally:

> **Everything the bundled Iyon product can do through the extension/product layer can also be done by third-party TypeScript through the same public APIs.**

And with this performance rule:

> **The TS layer may describe and orchestrate the UI, but actual terminal semantics, high-frequency layout/projection/history machinery, and native scrollback remain Rust.**

That is the target the implementation should optimize toward without reopening the top-level architecture.

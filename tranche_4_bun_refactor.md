# Tranche 4 — Bun TypeScript surfaces for `iyon:api` and `iyon:core`

## Mission

Expose the T3 provider-agnostic native kernel to Bun through the canonical
`iyon:api` and `iyon:core` virtual modules. `iyon:api` becomes the typed,
protocol-only TypeScript boundary for model requests, messages, content,
stream events, errors, and normalized metadata; `iyon:core` becomes the owned
native-session boundary for snapshots, events, model turns, tool lifecycles,
approvals, queues, and cancellation. The implementation must preserve the
native kernel's canonical transcript and event ordering while making the
ordinary extension path ergonomic (`AsyncIterable`, `AbortSignal`, and a thin
`AgentSession`) and keeping the arbitrary-agent path fully public through
`KernelSession`. The default Iyon agent, providers, tools, application, TUI,
and extension loader remain later TypeScript work.

## Prerequisites

- T1 is merged and green: Bun owns the process; `iyon-native` is a working
  napi-rs addon; the native loader and `iyon:api`/`iyon:core` virtual-module
  resolution seams exist; cancellation probes and the standalone-addon smoke
  path are available.
- T2 is merged and green: `tools/api-surface/` has the authoritative generated
  mapping schema and `packages/iyon-sdk/` has the generated/development
  declaration pipeline. T4 updates those mappings as each native capability
  lands; it must not create a competing handwritten coverage inventory.
- T3 is merged and green: `iyon-core` exposes the provider-agnostic kernel
  primitives, session snapshot/transcript model, event channel, model-turn
  lifecycle, tool-execution lifecycle, approval/queue operations, and explicit
  cancellation while retaining the Rust `IyonCore` compatibility façade. The
  kernel does not register built-in tools or call the old default agent loop.
- The current inventories (`iyon-api.md` and `iyon-core.md`) are migration
  contracts, not a reason to bind the current private `agent` modules directly.
  Where the current implementation has `run_model_turn`, `ToolCallAssembler`,
  `AgentMessage`, or `IyonCore` internals, T4 binds the public T3 extraction or
  wraps it; it does not recreate those algorithms in TypeScript or N-API.
- T4 must not assume T5's `iyon:tui` handles, T6's package/extension
  activation transaction, T7's TypeScript providers, T8's TypeScript tools, or
  T9's bundled agent. The synthetic agent in this tranche uses only the T4
  low-level API and a fake in-memory provider.

## Invariants (do not violate)

- Bun owns control flow. The intended path is
  `TypeScript → native Rust operation → Promise → TypeScript continuation`.
  Native code never calls arbitrary JavaScript providers, tools, hooks, or
  event callbacks.
- Every operation that can outlive one synchronous N-API call owns an explicit
  native cancellation token/handle. An abandoned or rejected JavaScript
  Promise is not cancellation. `AbortSignal` is an adapter that invokes the
  explicit native cancel operation and removes its listener when the operation
  settles.
- No Tokio future may retain borrowed N-API values or a JS-thread environment
  across an `.await`; convert incoming values to owned Rust data before
  starting async work. Rust failures and panics are returned as typed N-API
  errors and never casually unwind through Bun.
- The T2 generated mapping metadata remains authoritative for snake_case to
  camelCase names. The runtime may choose a TypeScript discriminated-union
  representation for Rust enums, but every such choice must be recorded in the
  mapping schema and reflected in generated declarations.
- The canonical protocol is `iyon:api`: `ModelApi`, `ModelRequest`,
  `ModelMessage`, `ContentBlock`, `ModelToolSpec`, `ModelStreamEvent`,
  `ModelParams`, `ModelMetadata`, `Usage`, `StopReason`, `ModelError`,
  `ModelErrorKind`, `ReasoningLevel`, and `CacheRetention`. Concrete providers
  remain Rust deprecated compatibility for now; they are not the long-term
  `iyon:api` contract and are not ported in T4.
- Native session state owns the canonical transcript, IDs, event order, and
  lifecycle validation. TypeScript façades may add ergonomics but may not
  maintain a second authoritative transcript or synthesize terminal events.
- Tool execution is TS-driven: native prepares and records a tool lifecycle,
  approval, queue, and result; native does not discover or invoke arbitrary JS.
- `KernelSession` is sufficient for arbitrary agents. `AgentSession` is only a
  thin convenience façade over public kernel operations; no privileged
  default-agent API may be introduced.
- The existing Rust `iyon-api`, `iyon-core`, `iyon-tui`, and compatibility
  application behavior remain buildable. T4 does not bind or modify
  `iyon-tui`, does not delete `crates/iyon`, and does not remove the existing
  provider implementations.

## Out of scope

- Replacing `ARCHITECTURE.md`; that belongs to T0 and is only a prerequisite
  document change, not a T4 implementation edit.
- WASM, WIT, Rhai, a custom JavaScript engine, an embedded Bun process, or a
  Rust-owned application/process loop.
- The `iyon:tui` value/handle/runtime/harness/trait surface (T5), including
  serializing or duplicating scrollback, layout, or terminal behavior.
- Package discovery, extension activation/rollback, contribution registries,
  bundled-vs-third-party loading, or plugin permissions (T6).
- Porting OpenRouter, OpenAI Codex, Mock, or any other provider to TypeScript
  (T7). Existing Rust provider constructors may remain a clearly deprecated
  compatibility disposition in T2 metadata and, if already needed by the
  compatibility product path, remain behind the existing native compatibility
  seam; they must not become new provider-registration logic in T4.
- Porting built-in tools or tool renderers (T8), the default agent algorithm
  and bundled agent package (T9), the default app/CLI (T10), TUI bridge
  optimization (T11), or packaging/ecosystem hardening (T12).
- New third-party dependencies, public Rust API redesign outside T3's kernel
  contract, or changes to the old Rust product's provider selection/auth
  behavior.

## Adjacent tranche contracts

T4 consumes T1's addon/virtual-module/cancellation seams, T2's generated
schema and SDK declaration pipeline, and T3's owned kernel handles. It exports
the stable TypeScript protocol declarations and runtime implementations for
`iyon:api` and `iyon:core`, the low-level `KernelSession`/turn/tool handles,
the thin `AgentSession`, explicit cancellation semantics, canonical snapshots,
and a reusable synthetic-agent fixture. T5 can consume the core event stream
without knowing N-API details; T6 can activate the same public modules for
extensions; T7/T8 can supply providers/tools through the protocol and tool
lifecycle; T9 can implement the bundled agent solely with `KernelSession`.

T4 must not steal the provider registration policy from T7, the tool
implementation/renderer policy from T8, the agent algorithm from T9, or any
TUI/native rendering responsibility from T5. If a later tranche needs a
placeholder, T4 provides only typed protocol records and lifecycle handles:
`ModelApi` is an interface that later providers implement, and tool execution
accepts a prepared request/result but never a native or privileged tool
implementation.

## Commits

### Commit 1 — Define the generated API and core TypeScript contracts

**Why:** The generated T2 schema must describe the public boundary before
native methods and façades are added. Establishing the value shapes first lets
Rust and TypeScript implementations be reviewed against one contract and
prevents accidental exposure of the current Rust private-agent model.

**Files:**

- `packages/iyon-sdk/src/api.ts`
- `packages/iyon-sdk/src/core.ts`
- `packages/iyon-sdk/src/index.ts`
- `packages/iyon-sdk/src/generated/**`
- `tools/api-surface/**` (only the T2 mapping inputs/generated records for the
  `iyon-api` and `iyon-core` dispositions)
- `packages/iyon-sdk/tests/api-contract.test.ts`

**Work:**

1. Extend the T2-generated declarations, rather than hand-maintaining a
   second inventory, with owned TypeScript types for every protocol item:
   `ModelRequest`, `ModelParams`, `ModelMetadata`, `ModelMessage`,
   `ContentBlock`, `ModelToolSpec`, `ModelStreamEvent`, `Usage`,
   `StopReason`, `ModelError`, `ModelErrorKind`, `ReasoningLevel`,
   `CacheRetention`, and the `ModelApi` provider interface. Use normal
   discriminated unions: for example `ModelMessage` uses `role`, content
   blocks and stream events use `type`, and tool-call fields are
   `toolCallId`, `toolName`, and `argumentsDelta`.
2. Map all Rust variants, including text/thinking start/delta/end, tool-call
   start/delta/end, usage, done, and error. Preserve optional IDs/names and
   `contentIndex`; do not collapse lifecycle events into text-only shortcuts.
   Use string unions for `ReasoningLevel`, `CacheRetention`, and `StopReason`
   with the exact wire spellings from the Rust protocol. Keep `ModelError.kind`
   and `message` available for typed error handling.
3. Define the core-facing declaration layer: branded or opaque ID aliases,
   `SessionSnapshot`, canonical transcript-entry/content shapes, `CoreEvent`,
   `ModelTurnOptions`, `ToolExecutionRequest`, `ToolResult`, queue/approval
   records, and the public method contracts for `KernelSession`, `ModelTurn`,
   `ToolExecution`, and `AgentSession`. The declarations must express
   `nextEvent(): Promise<CoreEvent | null>`, `events(): AsyncIterable<CoreEvent>`,
   `turn.push`, `turn.pushMany`, `finish`, `fail`, and explicit `cancel`.
4. Record the enum discriminant and field-name policies in the T2 mapping
   metadata, including any intentionally lazy TypeScript façade mapping. Mark
   `MockModelApi`, `OpenRouterModelApi`, and `OpenAICodexModelApi` as deprecated
   Rust compatibility dispositions rather than adding them to the protocol
   module.

**Tests / verification:**

- `bunx tsc --noEmit -p packages/iyon-sdk/tsconfig.json` (or the T2-provided
  SDK typecheck script) succeeds with no implicit `any` in the new declarations.
- `bun test packages/iyon-sdk/tests/api-contract.test.ts` asserts representative
  union narrowing, camelCase fields, optional tool-call metadata, and all
  protocol discriminants are accepted.
- Run the T2 API-surface check and assert `missing=0`, `stale=0`, and no new
  permanent `Ignore`/`Unsupported` disposition for a reachable protocol or
  kernel symbol.

**Must not:** Add runtime N-API calls, provider implementations, a second
  schema, Rust enum names such as `TextDelta` as the public discriminant, or
  an agent-only type that later T9 would need to access privately.

### Commit 2 — Add owned N-API protocol and error conversions

**Why:** Native entry points need one audited conversion layer before session
and operation handles are exposed. This is where serde/JSON-like JS values are
turned into owned Rust protocol values and where Rust failures receive stable
codes without leaking implementation details through N-API.

**Files:**

- `crates/iyon-native/src/lib.rs`
- `crates/iyon-native/src/api.rs`
- `crates/iyon-native/src/error.rs`
- `crates/iyon-native/src/value.rs`
- `crates/iyon-native/tests/api_conversion.rs`
- `tools/api-surface/**` (T2 mapping records for implemented protocol
  conversions)

**Work:**

1. Add owned conversion functions for every `iyon:api` input/output shape:
   model request fields, messages, content blocks, tool specs, params,
   metadata, stream events, usage, stop reasons, and model errors. Convert all
   strings, byte arrays, optional values, and JSON arguments before an async
   operation begins; never retain a borrowed `JsObject`, `JsUnknown`, or N-API
   environment in a Tokio future.
2. Export only the value/conversion operations needed by the later handles.
   Keep the protocol as plain structured values at the JS boundary; use owned
   native handles only for stateful sessions and operations. Ensure field names
   follow the generated metadata rather than ad hoc `serde(rename)` drift.
3. Define a stable native error envelope with an operation/code/kind/message
   distinction. Map `ModelErrorKind::Cancelled` and native cancellation to the
   same typed cancellation category, retain provider/transport/auth/request
   categories, and preserve unknown failures. Convert panics at the N-API
   boundary to an error result.
4. Keep the existing concrete provider Rust types untouched and record their
   deprecated compatibility status. Do not add a TypeScript provider factory;
   T7 will own provider registration and migration.

**Tests / verification:**

- `cargo fmt --check` and `cargo test -p iyon-native --test api_conversion`.
- Round-trip tests cover every protocol discriminant, camelCase field,
  optional tool-call identity, binary image data, JSON arguments, and the
  `Cancelled`/provider/transport error mappings.
- Assert malformed discriminants, missing required fields, invalid JSON, and
  oversized/unrepresentable values return typed errors rather than panic.
- `cargo check --workspace` still passes, and the T2 coverage report identifies
  these conversions as implemented.

**Must not:** Start model requests, retain JS callbacks, invoke providers or
tools, add a Rust HTTP layer, or serialize native session state as a substitute
for the handles planned in the next commits.

### Commit 3 — Bind native sessions, snapshots, events, and queues

**Why:** A stateful `KernelSession` is the ownership boundary for the canonical
transcript and event channel. It must exist before turns and tool handles can
be made safe, because those operations must attach to one session and one
native lifecycle.

**Files:**

- `crates/iyon-native/src/core.rs`
- `crates/iyon-native/src/core/session.rs`
- `crates/iyon-native/src/core/events.rs`
- `crates/iyon-native/src/core/handles.rs`
- `crates/iyon-native/src/lib.rs`
- `crates/iyon-native/tests/session.rs`
- `tools/api-surface/**` (T2 mappings for session/event/snapshot/queue methods)

**Work:**

1. Wrap the T3 public session primitive in an owned N-API handle. The handle
   must own or explicitly reference the native session for its entire lifetime,
   reject use after close, and be safe to retain across JS async continuations.
   Do not expose `AgentMessage`'s current private Rust representation directly.
2. Bind session construction/options, `snapshot()`, `appendMessage()`,
   `appendEntry()`, and the public queue operations. Preserve T3's IDs,
   timestamp/ordering rules, session metadata, model selection, reasoning,
   system prompt, and canonical transcript content in snapshots. Validate
   ownership and illegal lifecycle transitions in Rust.
3. Bind `nextEvent()` as a Promise resolving to an owned mapped `CoreEvent` or
   `null` on a closed stream. Keep the native receiver/channel behind the
   session handle; do not use a JS callback or one native call per event
   subscriber. Define the close/drop behavior so pending receivers and child
   operations terminate deterministically.
4. Bind steering/follow-up queue state as data operations only. A queued item
   must remain distinguishable from a committed transcript message until T3's
   lifecycle says it is delivered, matching the current `SteerQueued` behavior.

**Tests / verification:**

- `cargo test -p iyon-native --test session` creates two sessions, appends
  messages/entries, verifies independent IDs and exact snapshot transcript
  order, and observes mapped startup/queue events through `nextEvent()`.
- Assert a closed session, wrong child-session handle, and invalid append are
  typed errors; assert `nextEvent()` returns `null` exactly at stream closure.
- Run `cargo test -p iyon-core` and `cargo check --workspace` to verify the
  binding is wrapping T3's public kernel rather than reintroducing old agent
  internals.

**Must not:** Bind `iyon-tui`, expose a private `RuntimeState`, register built-in
tools, create a second transcript in N-API, or implement the default agent loop.

### Commit 4 — Bind model-turn handles and explicit cancellation

**Why:** The provider-agnostic model loop is the key Bun continuation path:
TypeScript owns the provider stream and pushes protocol events into a native
turn. This commit makes streaming, event ordering, terminal results, failure,
backpressure, and cancellation explicit without native-to-JS callbacks.

**Files:**

- `crates/iyon-native/src/core/model_turn.rs`
- `crates/iyon-native/src/core/events.rs`
- `crates/iyon-native/src/core/handles.rs`
- `crates/iyon-native/src/lib.rs`
- `crates/iyon-native/tests/model_turn.rs`
- `tools/api-surface/**` (T2 mappings for model-turn and cancellation methods)

**Work:**

1. Expose `session.beginModelTurn({ request })` using the T3 model-turn
   primitive. The request is converted to an owned `ModelRequest` once. Return
   a child `ModelTurn` handle with `push(event)`, `pushMany(events)`,
   `finish()`, `fail(error)`, and `cancel()`; all methods validate that the
   turn is open and belongs to its session.
2. Feed `ModelStreamEvent` values through the T3 extraction of the existing
   `run_model_turn`/assembler behavior. Preserve message-start/delta/finish,
   thinking, tool-call assembly, usage, stop reason, partial output on
   interruption, and terminal event ordering. `pushMany` is a bounded batch
   convenience, not a bypass around native validation.
3. Give every active turn an explicit cancellation token and operation state.
   Cancellation must interrupt a pending stream push or event-channel send,
   settle the turn once, preserve the native partial snapshot required by T3,
   and report the typed cancelled result/error consistently. `finish` and
   `fail` must race safely with cancellation and cannot emit duplicate terminal
   events.
4. Keep provider invocation outside native. A fake or future TypeScript
   provider supplies events; native consumes only owned `ModelStreamEvent`
   values. Do not adapt `ModelApi::stream` into a JS callback from Rust.

**Tests / verification:**

- `cargo test -p iyon-native --test model_turn` pushes a full fake sequence and
  asserts exact assistant events, canonical assistant transcript content,
  usage, stop reason, and tool-call assembly.
- Tests cover `pushMany`, malformed/out-of-order events, `fail`, double finish,
  cancellation during a blocked event send, cancellation after a tool call,
  and preservation of partial assistant content.
- Run `cargo test -p iyon-core`'s existing cancellation/backpressure tests and
  `cargo check --workspace`; assert no native test requires a JS callback or
  the default Iyon app.

**Must not:** Call a provider, read environment credentials, execute tools,
  expose a privileged agent turn, or treat Promise abandonment as native
  cancellation.

### Commit 5 — Bind tool execution, approvals, and lifecycle results

**Why:** T8 and T9 need a public way to drive tools from TypeScript while the
native kernel remains the authority for transcript/event consistency. The
handle must model preparation, approval, result, failure, queue interaction,
and cancellation without embedding any tool implementation.

**Files:**

- `crates/iyon-native/src/core/tool_execution.rs`
- `crates/iyon-native/src/core/approval.rs`
- `crates/iyon-native/src/core/queue.rs`
- `crates/iyon-native/src/core/handles.rs`
- `crates/iyon-native/src/lib.rs`
- `crates/iyon-native/tests/tool_execution.rs`
- `tools/api-surface/**` (T2 mappings for tool/approval/queue methods)

**Work:**

1. Expose `session.prepareToolExecution({ turnId, messageId, toolCallId,
   toolName, arguments })` returning an owned `ToolExecution` handle. Bind
   native preparation/start/update/result state and map the corresponding
   `CoreEvent` records, including structured details, error state, and
   termination state.
2. Bind explicit `approve(approvalId)`, `reject(approvalId, reason?)`,
   `cancel()`, `finish(result)`, and `fail(error)` operations with native
   one-shot validation. Keep approval IDs and tool-call IDs stable and reject
   stale, duplicate, or cross-session decisions.
3. Bind queue operations needed by ordinary extensions (`enqueue`, `dequeue`,
   `drain`, and inspection/snapshot as specified by T3). Preserve the current
   distinction between queued steering and committed user messages, and make
   cancellation leave the T3-defined queue/transcript state.
4. Map tool result content using `ContentBlock`; do not force arbitrary details
   through strings. Native records the result and emits events but never looks
   up a tool name, calls JS, or runs the old `register_builtin_defaults` path.

**Tests / verification:**

- `cargo test -p iyon-native --test tool_execution` covers prepare → approve →
  finish, reject with/without reason, structured result details, error result,
  cancellation, duplicate completion, stale approval, and cross-session
  handle rejection.
- Assert the canonical snapshot contains exactly one tool-result entry and
  event order is stable under approval and cancellation.
- Run the relevant T3 kernel tests, `cargo check --workspace`, and the T2
  coverage check; no builtin tool registry or Rust hook callback may be needed.

**Must not:** Add tool implementations, renderers, provider-specific tool
policy, JS callbacks, builtin registration, or a separate tool-plugin loader.

### Commit 6 — Implement the `iyon:api` and `iyon:core` TypeScript façades

**Why:** The native handles are intentionally explicit; ordinary extensions
need a safe TypeScript surface that owns subscriptions, abort listeners, and
camelCase ergonomics without hiding lifecycle semantics. This is also where
the two supported power levels become usable from Bun.

**Files:**

- `packages/iyon-runtime/src/modules/api.ts`
- `packages/iyon-runtime/src/modules/core.ts`
- `packages/iyon-runtime/src/modules/errors.ts`
- `packages/iyon-runtime/src/modules/abort.ts`
- `packages/iyon-runtime/src/modules/async-events.ts`
- `packages/iyon-runtime/src/virtual-modules/iyon-api.ts`
- `packages/iyon-runtime/src/virtual-modules/iyon-core.ts`
- `packages/iyon-runtime/src/index.ts`
- `packages/iyon-sdk/src/api.ts`
- `packages/iyon-sdk/src/core.ts`
- `packages/iyon-sdk/src/index.ts`
- `tools/api-surface/**` (facade dispositions and generated declaration links)
- `packages/iyon-runtime/tests/facades.test.ts`

**Work:**

1. Make `iyon:api` export the protocol types and `ModelApi` interface only.
   A provider implements `stream(request, options?)` as an async iterable (or
   an equivalent promise of one) and yields the exact generated
   `ModelStreamEvent` union. Add conversion only at the native boundary; do
   not add OpenRouter/Codex/Mock logic here.
2. Make `iyon:core` export `KernelSession`, `ModelTurn`, `ToolExecution`,
   snapshots, event types, and typed errors. Translate native `nextEvent()` to
   an ergonomic `events()` `AsyncIterable` that stops on `null`, propagates a
   typed native error, and does not poll or spin after closure.
3. Implement the low-level methods as direct wrappers over the native handles:
   `appendMessage`, `appendEntry`, `snapshot`, `beginModelTurn`,
   `prepareToolExecution`, queue methods, approval methods, `push`,
   `pushMany`, `finish`, `fail`, and explicit `cancel`. Keep ownership and
   invalid-state errors from native rather than duplicating them in JS.
4. Implement the thin `AgentSession` façade with `.prompt()`, `.steer()`,
   `.followUp()`, `.abort()`, `.setModel()`, `.setReasoning()`,
   `.setActiveTools()`, and `.events()`. It may compose queue/turn operations,
   but it must not contain the default agent loop and must not expose anything
   unavailable to an arbitrary `KernelSession` user.
5. Bridge an optional `AbortSignal` on every long-lived operation. If already
   aborted, do not start native work; otherwise register one listener that
   calls the explicit native `cancel`, remove it on resolve/reject/cancel, and
   preserve the native terminal event/result. Test both signal cancellation
   and explicit-handle cancellation.

**Tests / verification:**

- `bun test packages/iyon-runtime/tests/facades.test.ts` verifies the virtual
  modules resolve through the T1 loader, native errors narrow by code/kind,
  `events()` yields ordered events and terminates at `null`, and all method
  names match generated camelCase declarations.
- Assert abort listeners are removed after success, failure, and cancellation;
  an abandoned Promise without a signal does not claim to cancel native work.
- `bunx tsc --noEmit` across `packages/iyon-runtime` and `packages/iyon-sdk`
  proves the façade implements the generated SDK contracts with no `any` escape.
- `cargo check --workspace` ensures the runtime changes do not alter the Rust
  compatibility crates.

**Must not:** Add a hidden agent algorithm, native-to-JS callbacks, a second
  event source, TUI values/handles, provider registrations, or extension
  activation behavior.

### Commit 7 — Prove the synthetic agent and wire tranche verification

**Why:** The exit criterion is behavioral and cross-language: a completely
synthetic TypeScript agent must exercise the public surface without the Iyon
application. A focused fixture and CI gate prevent later tranches from
accidentally depending on a private native or default-agent path.

**Files:**

- `packages/iyon-runtime/tests/fixtures/synthetic-agent.ts`
- `packages/iyon-runtime/tests/synthetic-agent.test.ts`
- `packages/iyon-runtime/tests/cancellation.test.ts`
- `packages/iyon-sdk/tests/public-surface.test.ts`
- `tools/api-surface/**` (final T4 coverage assertions/report fixtures)
- `package.json` (only T4 verification scripts)
- `.github/workflows/ci.yml` (only T4 Bun/native binding jobs or steps)

**Work:**

1. Build a fake `ModelApi` in the fixture that emits `started`, text start/
   delta/end, a tool-call sequence, usage, and `done` without network access.
   The test must create a `KernelSession`, append a user message, build a
   request, begin a model turn, push the fake events, and observe the assistant
   events through `events()`.
2. Drive `prepareToolExecution`, a fake approval/result path, and finalization;
   inspect the canonical snapshot and assert user, assistant, tool-call, and
   tool-result records are in native order with the expected IDs/content.
3. Start a deliberately pending operation, cancel it through `AbortController`
   and through the explicit handle, and assert the native operation terminates
   once with the typed cancelled outcome. Include cancellation during event
   backpressure and after partial assistant output.
4. Assert the fixture imports only `iyon:api` and `iyon:core` public exports;
   it must not import `iyon`, `iyon:tui`, a concrete provider, builtin tools,
   an N-API implementation module, or an internal Rust symbol. Export the
   fixture helper for T9's custom-agent proof, but keep it a test fixture, not
   a bundled agent.
5. Add focused package scripts/CI steps for the Rust workspace check, native
   tests, SDK/runtime typecheck, Bun tests, and the T2 API-surface report.
   CI assertions are `missing=0`, `stale=0`, no coverage gap for the T4 public
   methods, and a green synthetic-agent test against the built `.node` addon.

**Tests / verification:**

- `test -f tranche_4_bun_refactor.md`.
- `cargo fmt --check && cargo check --workspace && cargo test -p iyon-native`.
- `bun install --frozen-lockfile` followed by the T1/T2-provided typecheck,
  `bun test packages/iyon-runtime/tests packages/iyon-sdk/tests`, and the
  API-surface coverage command.
- The synthetic test must pass against the actual N-API addon, not a mocked
  `native.ts`, and must satisfy every T4 exit criterion: create session, append
  user message, feed fake model events, observe assistant events,
  prepare/finish fake tool result, inspect canonical transcript, and cancel an
  active operation.

**Must not:** Add the default Iyon app/agent, port providers/tools, add TUI
  coverage, loosen CI with ignored failures, or hide a native failure behind a
  mock-only test.

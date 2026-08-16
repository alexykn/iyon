# Tranche 9 — Migrate the default agent to a bundled TypeScript extension

## Mission

Move the default Iyon agent algorithm out of privileged Rust behavior and into `plugins/agents/iyon/`, where it is an ordinary bundled TypeScript extension loaded through the same registration path as third-party agents. Port request construction, the default prompt, context selection, active-tool exposure, reasoning selection, model-turn continuation, stop-reason decisions, sequential tool execution, steering, follow-up behavior, and the per-turn tool-call policy while preserving current behavior. The implementation must be expressible solely with the public `iyon:core` low-level API (`KernelSession`, `beginModelTurn`, `prepareToolExecution`, and the other public session primitives); Rust continues to own canonical session history and native correctness, and T10 remains responsible for selecting this agent from the shipped app.

## Prerequisites

- T1 provides the Bun workspace/runtime, N-API addon loading, and virtual `iyon:*` modules.
- T2 provides the API-surface inventory and coverage checks.
- T3 provides the provider-agnostic kernel and has verified that kernel construction does not silently install Iyon product defaults.
- T4 provides the public `iyon:api` and `iyon:core` TypeScript façades, including the low-level `KernelSession`/model-turn/tool-preparation operations and explicit cancellation.
- T5 provides any required public stream/value types; T9 does not add TUI bindings.
- T6 provides the transactional extension loader and the `iyon.agents.register` contribution registry. Bundled and third-party packages use that same loader.
- T7 provides the TypeScript provider contributions and deterministic mock/scripted provider support.
- T8 provides the TypeScript tool contributions, active-tool registry, tool definitions, approvals/hooks façade, and public execution preparation used by an agent.

This tranche must not assume the T10 app, composer, footer, approval UI, or canonical Bun executable. The existing Rust application and its compatibility agent path may remain until T10; T9 must not cut over the shipped binary or delete `crates/iyon`.

## Invariants (do not violate)

- Bun/TypeScript owns the agent control flow and all product policy. There is no Rust-owned agent loop, Rust-owned subprocess, embedded Bun, WASM, Rhai, WIT, or private native agent capability.
- `plugins/agents/iyon/` may import only public `iyon:api`, `iyon:core`, `iyon:tui` where a public value is genuinely needed, `iyon:plugins`, and declared workspace SDK/type packages. It must not import `iyon-native`, Rust source paths, private N-API symbols, `crates/iyon-core`, or compatibility application modules.
- Rust owns canonical session history, message IDs, native model/tool operations, cancellation handles, and correctness-critical state. TypeScript does not maintain a second authoritative transcript or manually replay native history.
- Preserve the current request semantics: normalize an empty system prompt to absent, lower canonical user/assistant/tool-result messages while omitting status-only messages, expose only active tools in deterministic order, carry session metadata, and use the session’s selected reasoning effort.
- Preserve the current loop semantics: `ToolUse` with calls continues, `Stop`/`Length` without calls finishes, invalid stop/call combinations fail explicitly, provider errors fail, and aborted/interrupted streams preserve partial assistant output and cancel cleanly.
- Requested tools execute in provider order and are awaited one at a time. Do not introduce parallel tool execution merely because the public API permits it.
- The bundled Iyon policy rejects more than 16 requested tool calls in one model turn before starting any of them. A larger host/native safety ceiling, if retained, is not a replacement for this product policy.
- Steering is drained in arrival order only at natural turn boundaries; it does not abort an in-flight response. A batch that arrives before completion causes a follow-up response, and a queued message is never silently lost across cancellation/restart.
- Registration is ordinary extension registration: `iyon.agents.register({ id: "iyon", create(ctx) { return new IyonAgent(ctx); } })`. There is no `is_builtin` or privileged-default branch.
- No app rendering, approval presentation, composer behavior, footer behavior, or CLI startup behavior is moved into the agent package. The agent may await the public approval/execution operation; T10 owns how approval is presented.

## Out of scope

- T10: app/CLI migration, event-to-TUI presentation, composer and footer UX, provider selection UX, approvals UI, executable cutover, and removal/deprecation sequencing for the Rust binary.
- T7/T8: provider implementations, tool implementations, tool-specific renderers, tool registration policy, and the product’s tool presentation model.
- T11: N-API/TUI bridge optimization, batching, and benchmarks. T9 accepts the existing public bridge costs.
- T12: packaging polish, ecosystem documentation, release hardening, and permanent removal of deprecated Rust compatibility surfaces.
- Changes to `crates/iyon-core`’s private `agent/` implementation solely to make the old loop easier to call from TypeScript. The old `run_agent_loop`, `run_model_turn`, and private tool executor are behavioral oracles, not an API to wrap.
- New context compaction, summarization, retry policy, automatic parallelism, new tools, new providers, or new approval semantics.

## Adjacent tranche contracts

### Consumes from earlier tranches

T9 consumes the public low-level kernel/session contract from T4, the normal extension loader/registry from T6, and the TS provider/tool registries from T7–T8. It consumes cancellation and approval as explicit public operations; it does not reach through those façades to native internals.

### Exports for later tranches

T9 exports a registered `iyon` agent contribution, an independently runnable `IyonAgent` implementation, deterministic agent-policy tests, and public-agent conformance fixtures. T10 can select that contribution from the normal registry and connect its events to the app without changing the algorithm. The custom-agent fixture documents the minimum public API that future third-party agents may rely on, and the subagent proof establishes that independent sessions are supported.

### Must not steal from neighbors

T9 must not implement T10’s UI or executable cutover, T7/T8’s provider/tool work, or T11’s performance work. If an approval UI is not yet present, use a scripted public approval decision in tests and leave presentation to T10; do not add a temporary approval widget or a Rust callback shortcut. If T6 needs a registry stub for the `iyon` contribution, that stub belongs in the loader contract and must not become a T10 app-selection path.

## Commits

### Commit 1 — Scaffold the bundled-agent package and behavioral test harness

**Why:** Establish a buildable workspace package and deterministic public-API test support before porting policy. This makes later commits reviewable against scripted provider turns and prevents the implementation from depending on the old Rust loop as a test driver.

**Files:**

- `plugins/agents/iyon/package.json`
- `plugins/agents/iyon/tsconfig.json`
- `plugins/agents/iyon/test/support/scripted-provider.ts`
- `plugins/agents/iyon/test/support/session-driver.ts`
- `plugins/agents/iyon/test/support/fixtures.ts`
- `plugins/agents/iyon/test/support/*.test.ts`

**Work:**

- Declare the package as a normal Bun workspace package with the public `iyon:*` runtime imports and SDK types as workspace dependencies. Do not register an agent yet.
- Build a scripted model/provider test double that returns queued text, tool-call, stop, length, error, and aborted events through the public `iyon:api` provider surface. Keep event ordering and request snapshots observable.
- Build a session driver that creates a public `KernelSession`, supplies a scripted provider and T8 tool registry, drains public events, and exposes snapshots for assertions. It must not use private native handles or call the Rust loop.
- Add harness-level tests proving that a session can begin a model turn, receive a complete assistant result, and be cancelled through the public signal/operation path.

**Tests / verification:**

- `bun test plugins/agents/iyon/test/support`
- `bunx tsc -p plugins/agents/iyon/tsconfig.json --noEmit`
- Assert that scripted requests are captured, cancellation terminates a pending turn, and the harness imports only `iyon:api`/`iyon:core`/`iyon:plugins` public modules.

**Must not:**

- Add `IyonAgent`, agent registration, or any app startup wiring.
- Reuse `crates/iyon-core/src/agent/loop.rs` through an adapter.
- Add a second transcript store merely to simplify test assertions.

### Commit 2 — Port request, prompt, context, and reasoning policy

**Why:** Keep pure request policy separate from asynchronous turn execution. The policy is the highest-risk compatibility surface and can be reviewed independently with exact request snapshots.

**Files:**

- `plugins/agents/iyon/src/prompt.ts`
- `plugins/agents/iyon/src/context.ts`
- `plugins/agents/iyon/src/request.ts`
- `plugins/agents/iyon/src/reasoning.ts`
- `plugins/agents/iyon/test/policy.test.ts`

**Work:**

- Define typed internal policy inputs around the public `AgentContext`/`KernelSession` and public provider/tool definitions; avoid `any` or a private native type leak.
- Port the behavior of `crates/iyon-core/src/agent/request.rs::build_model_request`, `lower_messages`, `lower_message`, `lower_metadata`, and `non_empty_system_prompt` as TypeScript pure functions. Extract behavior; do not wrap or invoke those Rust functions.
- Keep the default system prompt in one named constant/builder and capture the current prompt text from the product behavior oracle. Make prompt composition deterministic and trim empty prompts to absence.
- Make context selection read canonical public history, retain the current full-message behavior, exclude status-only entries, preserve message/tool-call IDs, and leave a single explicit policy seam for future context-window work without adding compaction now.
- Source active tool definitions from the T8 public registry, not from Rust builtin registration. Preserve deterministic ordering and ensure inactive registered tools are absent from `ModelRequest.tools`.
- Map the public session/provider reasoning setting into `ModelParams.reasoning` exactly as current behavior does; do not move the T10 effort picker into this package.

**Tests / verification:**

- `bun test plugins/agents/iyon/test/policy.test.ts`
- Test names: `default_prompt_is_stable`, `empty_prompt_is_omitted`, `request_omits_status_messages`, `request_preserves_tool_results_and_metadata`, `request_exposes_active_tools_only`, `request_uses_session_reasoning_effort`, and `context_selection_preserves_canonical_order`.
- Snapshot a complete request for a mixed user/assistant/status/tool-result history and assert exact tools, metadata, reasoning, and message order.
- `bunx tsc -p plugins/agents/iyon/tsconfig.json --noEmit`

**Must not:**

- Add a TS-owned authoritative history or silently mutate the session while building a request.
- Invent a new default prompt, context truncation rule, retry rule, or reasoning selection UI.
- Import private Rust request builders or the native addon directly.

### Commit 3 — Port public model-turn streaming and outcome assembly

**Why:** Isolate provider-stream interpretation from the outer agent loop. This commit turns public `beginModelTurn` results into a typed TS outcome while preserving partial-message and tool-call behavior.

**Files:**

- `plugins/agents/iyon/src/turn.ts`
- `plugins/agents/iyon/src/stream.ts`
- `plugins/agents/iyon/test/turn.test.ts`

**Work:**

- Implement `runProviderTurn(ctx, request)` by calling the public `beginModelTurn` operation with the context’s abort/cancellation signal. Keep all native handles scoped to their public operation and release them deterministically.
- Assemble streamed assistant text, reasoning/thinking deltas, usage, and tool-call start/delta/end records into a typed `ModelTurnResult`. Preserve invalid/incomplete tool-call information so the outer policy can produce the same invalid-call result rather than dropping it.
- Map public stop reasons to `completed`, `interrupted`, or `failed` outcomes. On abort or interruption, return the partial assistant content that the kernel has recorded; do not synthesize a replacement assistant message in TS.
- Emit or forward only public core events needed by the host; do not add T10-specific rendering events.
- Treat provider-stream errors as explicit failures with context. Do not swallow errors or turn them into a normal stop.

**Tests / verification:**

- `bun test plugins/agents/iyon/test/turn.test.ts`
- Test names: `turn_assembles_text_and_reasoning_deltas`, `turn_assembles_multiple_tool_calls_in_order`, `turn_returns_partial_assistant_on_abort`, `turn_preserves_length_stop`, `turn_rejects_provider_stream_error`, and `turn_reports_invalid_tool_call_without_dropping_it`.
- Assert that a request is started through `beginModelTurn`, that cancellation reaches the public operation, and that no private native symbol is referenced.
- `bunx tsc -p plugins/agents/iyon/tsconfig.json --noEmit`

**Must not:**

- Call or wrap `crates/iyon-core/src/agent/turn.rs::run_model_turn`.
- Reimplement provider HTTP, native stream transport, or TUI rendering in TypeScript.
- Decide whether to continue after a turn; that belongs to the policy/loop commit.

### Commit 4 — Port stop decisions and sequential tool execution policy

**Why:** Make the product’s tool-call behavior explicit and test the safety-sensitive boundaries before composing the full agent loop.

**Files:**

- `plugins/agents/iyon/src/stop.ts`
- `plugins/agents/iyon/src/tools.ts`
- `plugins/agents/iyon/test/stop-and-tools.test.ts`

**Work:**

- Port `classify_stop_reason` from `crates/iyon-core/src/agent/loop.rs` as a typed, explicit decision function. Preserve the error cases for tool-use without calls and stop/length with calls.
- Implement the default `MAX_TOOL_CALLS_PER_MODEL_TURN = 16` policy in the bundled agent package. Validate the complete request before executing any tool so an over-limit response has no partial side effects.
- Resolve calls against the T8 public active-tool registry and use `prepareToolExecution` for each call. Pass the current public session, turn, call ID, workspace/cwd, approval façade, hooks, and abort signal through the public preparation/execution object.
- Execute with a plain ordered `for...of`/`await` sequence. Preserve invalid-call results, blocked/approval-rejected results, tool updates, cancellation, and final tool-result messages through public kernel APIs.
- Port the observable behavior of `execute_tool_requests`/`execute_one_tool_request` from the Rust loop as orchestration only. The native tool executor, hook implementation, filesystem safety, and T8 tool bodies remain owned by their existing layers.

**Tests / verification:**

- `bun test plugins/agents/iyon/test/stop-and-tools.test.ts`
- Test names: `tool_use_with_calls_continues`, `stop_without_calls_finishes`, `length_without_calls_finishes`, `tool_use_without_calls_fails`, `stop_with_calls_fails`, `sixteen_tool_calls_are_allowed`, `seventeenth_tool_call_is_rejected_before_execution`, `tool_calls_execute_in_provider_order`, `inactive_tool_call_is_reported_as_invalid`, `approval_and_cancellation_use_public_execution`, and `tool_result_is_committed_by_the_kernel`.
- Use two blocking scripted tools to assert that the second tool cannot start before the first completes; explicitly assert there is no `Promise.all`/parallel execution path.
- `bunx tsc -p plugins/agents/iyon/tsconfig.json --noEmit`

**Must not:**

- Raise the product limit to match a larger native ceiling.
- Add parallel execution, a new approval policy, tool-specific UI, or a second tool registry.
- Reach into T8 tool implementations or Rust `ToolRegistry` internals.

### Commit 5 — Compose steering, continuation, follow-up, and `IyonAgent.run`

**Why:** Assemble the pure policy and public operations into the complete default algorithm while keeping turn-boundary behavior and cancellation semantics visible in one focused review.

**Files:**

- `plugins/agents/iyon/src/steering.ts`
- `plugins/agents/iyon/src/continuation.ts`
- `plugins/agents/iyon/src/agent.ts`
- `plugins/agents/iyon/test/agent.test.ts`

**Work:**

- Implement `IyonAgent` with injected `AgentContext` and a `run(ctx): Promise<void>` method whose main loop is `while (!ctx.signal.aborted)`: build the next request, run one public model turn, apply stop policy, and execute requested tools.
- Port `drain_steers` and `inject_steered_messages` behavior using the public steering/session primitives. Drain all queued steering messages in order at turn boundaries, deliver them as canonical user messages, and never hold an async queue lock across an await.
- If steering arrives while a model response is streaming, let that response finish, then include the complete queued batch in the next request. If a completed response has pending steering, continue instead of emitting a final stop.
- Preserve interrupted assistant output and let the kernel’s canonical session history drive a later follow-up. Do not append duplicate assistant/tool messages from TypeScript.
- Keep provider/model selection, reasoning, active tools, workspace, approvals, and cancellation on `AgentContext`; do not reach into application state.
- Expose a small testable `buildRequest`/`shouldContinue`/`executeRequestedTools` composition internally, but keep the public contribution as the normal `Agent` contract from T6.

**Tests / verification:**

- `bun test plugins/agents/iyon/test/agent.test.ts`
- Test names: `run_composes_request_turn_and_tools`, `steering_is_drained_in_arrival_order`, `steering_does_not_abort_inflight_response`, `stop_with_pending_steering_continues`, `cancel_preserves_partial_history`, `follow_up_reuses_kernel_history`, `tool_results_feed_the_next_request`, and `run_stops_on_abort`.
- Assert request snapshots across a multi-turn scripted conversation, including a turn with tools, an interrupted partial response, and a steered follow-up.
- `bunx tsc -p plugins/agents/iyon/tsconfig.json --noEmit`
- `cargo check --workspace` to prove the Rust compatibility path remains buildable and untouched by the TS agent implementation.

**Must not:**

- Add app event reducers, TUI components, composer handling, or CLI commands.
- Call the old Rust `run_agent_loop`; this is a new implementation over the public façade.
- Start more than one model turn or tool execution concurrently for one agent run.

### Commit 6 — Register the bundled Iyon agent through the normal extension loader

**Why:** Make the completed implementation a real bundled contribution and prove that its activation path is identical to third-party agent activation.

**Files:**

- `plugins/agents/iyon/src/index.ts`
- `plugins/agents/iyon/package.json`
- `plugins/agents/iyon/test/registration.test.ts`
- `packages/iyon-runtime/test/agent-registration.integration.test.ts`

**Work:**

- Export the package entry point that performs exactly `iyon.agents.register({ id: "iyon", create(ctx) { return new IyonAgent(ctx); } })` through the T6 registration API.
- Keep `IyonAgent` construction dependency-injected; no singleton native core, global provider, hidden builtin registry, or app-specific context is allowed.
- Load the package through the same workspace/package activation path used for a third-party agent. Verify duplicate IDs and explicit replacement follow T6’s transactional registration rules.
- Keep this registration available for T10 to select, but leave the current shipped Rust binary’s startup and agent selection unchanged.

**Tests / verification:**

- `bun test plugins/agents/iyon/test/registration.test.ts packages/iyon-runtime/test/agent-registration.integration.test.ts`
- Assert `iyon.agents.get("iyon")` is created through the normal loader, has no builtin-only branch, and can run against the scripted provider/session driver.
- Assert activation rollback leaves no partial `iyon` registration when package initialization fails.
- `bunx tsc -p plugins/agents/iyon/tsconfig.json --noEmit && bunx tsc -p packages/iyon-runtime/tsconfig.json --noEmit`

**Must not:**

- Wire the contribution into `crates/iyon/src/main.rs`, the Rust TUI, or the T10 app.
- Add a second loader for bundled agents.
- Make the registration itself responsible for provider detection, auth, menus, approvals, or rendering.

### Commit 7 — Add custom-agent, subagent, and privileged-boundary proofs

**Why:** The exit criterion is architectural, not just behavioral: a materially different third-party agent must be able to use the same low-level public API, and an agent must be able to run an independent child agent/session without private capabilities.

**Files:**

- `packages/iyon-runtime/test/fixtures/custom-agent.ts`
- `packages/iyon-runtime/test/fixtures/subagent.ts`
- `packages/iyon-runtime/test/custom-agent.integration.test.ts`
- `packages/iyon-runtime/test/subagent.integration.test.ts`
- `plugins/agents/iyon/test/public-boundary.test.ts`
- `.github/workflows/ts-agent.yml`

**Work:**

- Add a fixture agent registered through the ordinary contribution API. It must differ materially from Iyon: compose a different system/request shape, expose a strict subset of tools, transform context before request construction through public history APIs, and stop after its own custom condition rather than Iyon’s continuation rule.
- Make the fixture call only `KernelSession`, `beginModelTurn`, `prepareToolExecution`, public `iyon:api` model types, and public registry/session operations. Add a source-level import/dependency assertion that rejects private native/Rust paths.
- Add a subagent fixture/test in which a parent agent creates a second independent public `KernelSession`, constructs/runs another registered agent, and then resumes. Assert distinct session IDs, isolated histories, independent provider requests, and no use of a global/private agent handle.
- Add the TypeScript CI workflow (or extend the T1 TS workflow only if that exact workflow already owns these checks) to run the agent package/runtime type checks, focused tests, forbidden-import check, and `cargo check --workspace`. Keep CI changes limited to T9’s agent proof.
- Record the final proof in test names and failure messages so a future change that makes Iyon special fails at review/CI rather than only during T10 integration.

**Tests / verification:**

- `bun test packages/iyon-runtime/test/custom-agent.integration.test.ts packages/iyon-runtime/test/subagent.integration.test.ts plugins/agents/iyon/test/public-boundary.test.ts`
- Test names: `custom_agent_uses_only_public_core_and_differs_from_iyon`, `custom_agent_can_select_a_subset_of_tools`, `custom_agent_can_transform_context`, `custom_agent_has_a_custom_stop_condition`, `agent_can_run_an_independent_subagent_session`, `subagent_history_is_isolated`, and `bundled_iyon_has_no_privileged_imports`.
- `bunx tsc -p plugins/agents/iyon/tsconfig.json --noEmit && bunx tsc -p packages/iyon-runtime/tsconfig.json --noEmit`
- `cargo check --workspace`
- `git diff --check`

**Must not:**

- Use a test-only private API, mock native handle, or Rust-owned agent loop to make the proof pass.
- Add parent/child UX, subagent product policy, recursive orchestration limits, or T10 app behavior.
- Treat passing Iyon-only tests as sufficient; the custom-agent and subagent proofs are required exit criteria.


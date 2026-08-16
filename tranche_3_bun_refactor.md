# Tranche 3 — Refactor iyon-core into a native execution kernel

## Mission

Turn `iyon-core` into a provider-agnostic Rust kernel that owns canonical session/transcript state, native identifiers, normalized model turns, assembled tool calls, tool and approval lifecycles, cancellation, bounded event delivery, and distinct prompt/steer/follow-up queues. Keep the existing `IyonCore`, commands, events, built-in tools, and Rust application behavior working through an explicit compatibility driver, while making the new kernel path constructible with injected registries and no implicit `register_builtin_defaults` call. This tranche creates the public Rust primitives that T4 will bind to TypeScript; it does not make Rust own the agent policy or the Bun process.

## Prerequisites

- T0 has replaced the old architecture description with the Bun + TypeScript + N-API handoff and frozen the dependency boundaries.
- T1 has established the Bun/native workspace shape and a buildable `iyon-native` viability slice. T3 does not depend on `iyon-native` implementation details.
- T2 has established the API-surface inventory and its missing/stale checks. Every new public kernel item introduced here must receive a real inventory disposition; no permanent ignore entry is acceptable.
- The current Rust workspace, `iyon-api`, `iyon-core`, `iyon-tui`, and compatibility `iyon` crate are green before each commit.
- T3 may use the existing `iyon-api` protocol types (`ModelMessage`, `ModelStreamEvent`, `ContentBlock`, `Usage`, and `StopReason`) but must not add provider-specific behavior to the kernel.
- T3 must not assume T4's TypeScript surfaces, T6's extension loader, T7's TypeScript providers, T8's TypeScript tools, or T9's TypeScript agent. Rust tests must drive the new primitives directly.

## Invariants (do not violate)

- `iyon-core` is a provider-agnostic native execution kernel, not the Iyon agent. The kernel has no default system prompt, provider selection UX, provider implementation, default continuation policy, reasoning cycling policy, context compaction policy, TUI code, or arbitrary-JS callback path.
- The Rust graph remains `iyon-api → iyon-core`; `iyon-tui` remains independent of both. Do not add `iyon-core` or `iyon-api` dependencies to `iyon-tui`.
- IDs are native-owned. Session, turn, message, tool-call fallback, and approval identifiers must be allocated and validated by native state, including the assistant message ID and late-provider-ID reconciliation.
- Canonical transcript state is separate from model request construction. Appending/querying canonical entries must not require constructing a request, and filtering, compaction, or RAG must not destructively mutate canonical history.
- `SessionEntry::Custom` namespaces are validated package-style identifiers. Unknown custom entries remain in snapshots/history but are not silently converted into model messages.
- Model normalization preserves assistant message identity, text/thinking accumulation, content ordering, usage, partial replies, Start/Delta/End semantics, tool draft identity `{message_id, content_index}`, late provider tool IDs, bounded event ordering, and cancellation behavior.
- A model-turn primitive consumes normalized provider events; it never calls a provider. The existing `run_model_turn` remains only as a compatibility adapter that obtains a stream from `ModelApi` and feeds the primitive.
- Native tool state transitions are validated and remain `Preparing → Prepared → Running → PendingApproval → Running → Finished / Failed / Cancelled`. The kernel never reports success without a real result and never executes arbitrary JavaScript.
- Approval requirement/policy is an input to the lifecycle, not a kernel decision. `ApprovalId`, pending state, waiting, resolution, cancellation, events, and invalid-transition errors are native. The `sudo` special case must not appear in the new kernel primitive.
- New kernel construction receives/injects its registry, policy inputs, channels, and resource limits. It must not call `register_builtin_defaults`. Legacy built-ins remain available only through the compatibility driver until T8.
- Prompt, steer, follow-up, and abort are distinct native operations. Existing public `SubmitTurn` remains a compatibility wrapper for Rust callers; it is not the long-term semantic API.
- Cancellation is explicit and preserves the currently defined partial-state semantics: in-flight assistant partials and completed tool results survive a normal interrupt; teardown may hard-abort only where the existing compatibility contract already permits it.
- Bounded event/command channels remain cancellation-aware under backpressure. Native tasks cannot outlive the state or runtime they reference.
- The existing compatibility `IyonCore` tests and public application tests remain green. The current 16-tool check may remain only in the compatibility/default-agent path; it is not a kernel invariant or product-wide core ceiling. Any larger native ceiling is documented as a host safety ceiling.

## Out of scope

- No N-API exports, `iyon-native` bindings, `iyon:api`/`iyon:core` TypeScript packages, Bun loader work, or standalone executable work; those belong to T4 and later.
- No provider migration or provider implementation work. `iyon-api` provider types remain available as deprecated compatibility inputs where necessary; the kernel does not select or call providers.
- No migration of built-in tools to plugins/TypeScript, no removal of `crates/iyon-core/src/tools/builtin/**`, and no plugin registry/extension activation work; tool migration belongs to T8 and extension activation to T6.
- No default-agent continuation, request-composition policy, reasoning cycling policy, 16-tools-per-turn product policy, context pruning, compaction, or RAG policy. The compatibility driver may preserve current behavior behind explicit legacy adapters.
- No `iyon-tui` changes, renderer changes, scrollback changes, or conversation UI redesign.
- No Rust-owned process, embedded Bun, WASM, WIT, Rhai, custom JS engine, or Rust HTTP/provider layer.
- No casual deletion of current public Rust commands, events, constructors, identifier types, hooks, or output helpers.

## Adjacent tranche contracts

### Consumes from earlier tranches

T3 consumes the frozen architecture/dependency rules from T0, the buildable Bun/native workspace boundary from T1, and the API-surface accounting mechanism from T2. It consumes the existing `iyon-api` stream/message protocol and the current compatibility test behavior, not a new provider contract.

### Exports for later tranches

T3 exports a documented public Rust kernel surface under `iyon_core::kernel` (with deliberate root re-exports where compatibility or binding ergonomics require them): native ID allocation, `KernelSession`, `SessionEntry`, `SessionSnapshot`, append/query operations and mechanical model-message lowering; the provider-independent `ModelTurn` push/finish/cancel primitive and its assembled tool-call outcome; validated tool/approval lifecycle handles and events; explicit steering/follow-up queues and prompt/steer/follow-up/abort operations; injected kernel construction and registry configuration; cancellation and bounded-channel contracts; and a compatibility `IyonCore` that still exposes the existing Rust-facing API. T4 binds these Rust primitives; it must not infer behavior from private compatibility-driver helpers.

### Must not steal from neighboring tranches

T3 must not add N-API or TypeScript bindings (T4), implement extension activation (T6), migrate providers (T7), move tools to plugins (T8), or define the default agent/product policies (T9/T10). If later work needs an absent behavior, T3 may expose a neutral native state/operation or a typed stub, but it must not implement the later policy in the kernel.

## Commits

### Commit 1 — Add canonical kernel sessions and transcript entries

**Why:** The current `SessionState.messages` vector and private `AgentMessage` type are coupled to the compatibility loop and to request construction. Later TS agents need a stable native session that can retain messages and namespaced extension data without destructive filtering.

**Files:**

- `crates/iyon-core/src/kernel/mod.rs` (new)
- `crates/iyon-core/src/kernel/session.rs` (new)
- `crates/iyon-core/src/lib.rs`
- `crates/iyon-core/src/session/mod.rs`
- `crates/iyon-core/src/session/state.rs`
- `crates/iyon-core/src/agent/mod.rs`
- `crates/iyon-core/src/agent/transcript.rs`
- `crates/iyon-core/src/agent/request.rs`
- `crates/iyon-core/src/ids.rs`

**Work:**

- Add `KernelSession`, `SessionEntry`, and `SessionSnapshot` in the new public kernel module. `SessionEntry` must include `Message(AgentMessage)` and `Custom { namespace: String, data: serde_json::Value }`; snapshots must be owned, read-only values suitable for crossing an async/native boundary later.
- Make `AgentMessage` and the message content needed by the kernel reachable through the deliberate kernel export. Preserve the existing root `SessionState`, `ModelSelection`, identifier re-exports, and field-level compatibility contract while introducing an explicit conversion/delegation boundary rather than deleting or silently changing them.
- Give `KernelSession` native ownership of session/message ID allocation and append operations. Provide the conceptual operations `entries`, `messages`, `append_message`, `append_entry`, `snapshot`, and `to_model_messages`; make allocation monotonic and reject an externally supplied conflicting message ID instead of silently renumbering history.
- Validate custom namespaces at `append_entry` using a package-style rule (non-empty segments, identifier-safe characters, no whitespace or path traversal). Return a typed error naming the invalid namespace. Preserve valid custom data in canonical entries and snapshots.
- Keep `to_model_messages` mechanical: lower only model-visible `User`, `Assistant`, and `ToolResult` entries; omit status messages and custom entries without deleting them. Do not include system prompts, tool definitions, provider selection, or reasoning policy in this method.
- Refactor `agent/request.rs` so its existing request adapter obtains messages from the kernel lowering operation while continuing to own compatibility-only system-prompt trimming, metadata, reasoning params, and tool-spec composition.
- Keep the old `SessionState` behavior available to existing `IyonCore` and hook callers through a compatibility view/conversion. Do not make two independently mutated canonical transcripts; document and test which object owns mutations.

**Tests / verification:**

- Add focused unit tests named `append_message_allocates_native_id`, `append_entry_rejects_invalid_namespace`, `snapshot_preserves_custom_entries`, `messages_query_excludes_custom_and_status_entries`, `to_model_messages_does_not_mutate_canonical_entries`, and `compatibility_session_state_round_trips_without_losing_metadata`.
- Assert that message IDs survive snapshots, custom entries survive a model-message projection, model message ordering matches canonical order, and request construction still preserves current system prompt/metadata behavior.
- Run `cargo fmt --all -- --check`, `cargo test -p iyon-core session`, and `cargo check --workspace`.

**Must not:**

- Do not add model/provider calls, request continuation, compaction/filtering mutation, TS types, N-API exports, or a new default prompt.
- Do not remove `SessionState`, `AgentMessage::id`, the existing root exports, or any current public constructor used by `crates/iyon`.

### Commit 2 — Extract provider-event model-turn normalization

**Why:** `agent/turn.rs::run_model_turn` currently combines provider I/O, stream normalization, event emission, accumulation, and cancellation. T4 needs the normalization state machine without a Rust-owned provider call.

**Files:**

- `crates/iyon-core/src/kernel/model_turn.rs` (new)
- `crates/iyon-core/src/lib.rs`
- `crates/iyon-core/src/kernel/mod.rs`
- `crates/iyon-core/src/agent/mod.rs`
- `crates/iyon-core/src/agent/turn.rs`
- `crates/iyon-core/src/agent/tool_call.rs`
- `crates/iyon-core/src/event.rs`

**Work:**

- Move the stateful normalization responsibilities out of `run_model_turn` into public `ModelTurn`/configuration/result types. The primitive begins an assistant message with the supplied native `TurnId` and `MessageId`, accepts `push(event)` and `push_many(events)`, and exposes `finish`, `fail`, and `cancel` transitions.
- Reuse or move `ToolCallAssembler`, `PendingToolCall`, `ToolCallRequest`, `AssembledToolCall`, and invalid-call reporting into the kernel-facing module. Keep content-index identity stable, create exactly one draft for delta-before-start, bind IDs that arrive late to the existing draft, and generate a deterministic fallback ID only when the provider never supplies one.
- Preserve the current normalization rules: text and thinking are accumulated into ordered `ContentBlock`s, thinking/text are flushed before a tool-call boundary, tool-call Start/Arguments/End deltas are emitted in order, usage is retained, and `MessageFinished` seals both complete and interrupted partial assistant messages.
- Make `push` consume `iyon_api::ModelStreamEvent` values but never invoke `ModelApi`. Keep event delivery bounded and race each awaited send against the cancellation token so backpressure cannot mask cancellation.
- Reduce `run_model_turn` to a compatibility adapter: call `ModelApi::stream`, feed each provider event to `ModelTurn`, then map the public kernel outcome to the existing compatibility `ModelTurnOutcome`. Provider stream errors remain failures; provider `Done { stop_reason: Aborted }` and explicit cancellation retain current partial-message semantics.
- Preserve existing `CoreEvent`, `MessageDelta`, and `ToolCallDelta` variants and ordering. Add only additive kernel event/result data when a direct primitive needs it, with a compatibility mapping rather than replacing old event shapes.

**Tests / verification:**

- Move the current `turn.rs` behavioral tests behind the adapter where appropriate and add direct primitive tests named `push_many_preserves_event_order`, `push_accumulates_text_and_thinking`, `tool_draft_identity_survives_late_provider_id`, `delta_before_start_emits_one_draft`, `finish_preserves_usage_and_assistant_id`, `cancel_preserves_partial_text_and_thinking`, `fail_does_not_fake_a_successful_result`, and `cancellation_honored_under_event_backpressure`.
- Assert the direct primitive can be driven entirely by a vector/channel of `ModelStreamEvent` values, with no `ModelApi` implementation involved, and that compatibility `run_model_turn` produces byte-for-byte equivalent text/tool event ordering to the current tests.
- Run `cargo fmt --all -- --check`, `cargo test -p iyon-core agent::turn`, `cargo test -p iyon-core kernel::model_turn`, and `cargo check --workspace`.

**Must not:**

- Do not put `ModelApi`, provider selection, retry logic, request construction, agent continuation, or tool execution in `ModelTurn`.
- Do not change provider stream semantics, tool-call identity, partial-reply retention, or existing public event meanings to simplify the extraction.

### Commit 3 — Add validated tool and approval lifecycle primitives

**Why:** `agent/tool_execution.rs` currently mixes tool lookup, hooks, approval policy, execution, event emission, and result-message construction. The kernel needs the lifecycle and approval correctness independently of any executor or policy.

**Files:**

- `crates/iyon-core/src/kernel/tool.rs` (new)
- `crates/iyon-core/src/kernel/approval.rs` (new)
- `crates/iyon-core/src/kernel/mod.rs`
- `crates/iyon-core/src/lib.rs`
- `crates/iyon-core/src/ids.rs`
- `crates/iyon-core/src/event.rs`
- `crates/iyon-core/src/agent/control.rs`

**Work:**

- Define public native tool-call/lifecycle data around the assembled call from Commit 2. The lifecycle handle must expose validated transitions for `prepare`, `prepared`, `start`, `request_approval`, `resolve_approval`, `finish`, `fail`, and `cancel`, with typed errors for stale IDs, duplicate resolution, and illegal transitions.
- Represent the exact state graph `Preparing → Prepared → Running → PendingApproval → Running → Finished / Failed / Cancelled`. Preserve the call identity, prepared arguments, result/error details, cancellation reason, and timestamps/sequence information needed to explain transitions without making the kernel responsible for rendering.
- Define an `ApprovalRequirement`/policy input and an `ApprovalState`/handle whose native ID, pending request, waiting state, resolution, cancellation, and transition events are owned by the kernel. Approval requirement must be passed in by the caller; it must not inspect tool names, shell commands, or `sudo`.
- Add additive core lifecycle/approval events or a typed kernel-event projection while retaining the existing `ToolCallStarted`, `ToolResult*`, and `ToolApproval*` compatibility events. Event emission must describe actual transitions; no `Finished` event may be emitted for a pending, failed, or cancelled execution.
- Keep `ToolExecutor`, `ToolContext`, `ToolResult`, hooks, and workspace/process I/O as executor-side contracts. This commit should provide the state machine and event sink adapters only; it should not execute a tool.
- Keep `AgentLoopControl` as a compatibility-only approval transport and make it capable of carrying the native `ApprovalId` without changing existing command behavior.

**Tests / verification:**

- Add unit tests named `tool_lifecycle_accepts_prepared_running_and_finished`, `approval_can_return_to_running_only_after_matching_resolution`, `approval_rejection_finishes_as_rejected_failure`, `cancel_is_terminal_and_idempotent`, `invalid_tool_transition_returns_typed_error`, `stale_approval_resolution_is_rejected`, and `lifecycle_events_match_state_transitions`.
- Assert that no executor callback is required to create or transition the lifecycle, an unresolved approval cannot finish, cancellation wakes a pending approval, and the same `ApprovalId` cannot resolve twice.
- Run `cargo fmt --all -- --check`, `cargo test -p iyon-core kernel::tool`, `cargo test -p iyon-core kernel::approval`, and `cargo check --workspace`.

**Must not:**

- Do not execute arbitrary Rust/JS tools from the kernel state machine, infer approval from `sudo`, or treat approval as a UI-only flag.
- Do not move built-in tool implementations, file/process code, tool renderers, or TUI code.

### Commit 4 — Make kernel construction explicit and registry-injected

**Why:** `RuntimeState::new` currently creates a `ToolRegistry` and unconditionally calls `register_builtin_defaults`, which would make a future TS-owned kernel silently depend on legacy Rust tools.

**Files:**

- `crates/iyon-core/src/kernel/kernel.rs` (new)
- `crates/iyon-core/src/kernel/mod.rs`
- `crates/iyon-core/src/lib.rs`
- `crates/iyon-core/src/tools/mod.rs`
- `crates/iyon-core/src/tools/registry.rs`
- `crates/iyon-core/src/runtime.rs`
- `crates/iyon-core/src/handle.rs`

**Work:**

- Add a public kernel construction/configuration type (for example `KernelConfig` and `Kernel`) that requires/injects a canonical session, tool registry/definitions, approval-policy input, event/command capacities, cancellation ownership, and optional host safety ceilings. Make empty/no-tool construction valid.
- Keep the kernel’s registry API provider/tool implementation agnostic: registration, lookup, active definitions, model specs, and explicit replacement/error behavior are separate from legacy default population. Do not make `ToolRegistry::new` or `Kernel::new` call any built-in registration method.
- Move the current built-in registration call to a clearly named compatibility-only constructor/helper used by `IyonCore::spawn*`. Make it explicit in the call graph that only this path invokes `register_builtin_defaults`; the new kernel path must not reach it.
- Preserve `IyonCore::spawn`, `spawn_with_owned_runtime`, `spawn_on_current_runtime`, selection constructors, hook injection, event capacities, and runtime ownership. Adapt their internal construction to pass a compatibility registry into the new kernel/runtime configuration rather than letting the kernel create one.
- Keep `register_builtin_defaults` and current legacy built-ins available to the compatibility driver until T8. If visibility must change, preserve an equivalent compatibility entry point and update the API inventory rather than deleting the behavior.
- Document any kernel-wide hard ceiling as a host safety ceiling. Do not encode the product/default-agent 16-tool policy in the kernel constructor.

**Tests / verification:**

- Add `kernel_construction_uses_only_injected_registry`, `empty_injected_registry_has_no_implicit_builtins`, `compatibility_spawn_explicitly_registers_legacy_builtins`, and `default_spawn_and_equivalent_builder_have_identical_events`.
- Assert that an injected registry is the exact source of tool definitions/model specs, a new kernel with an empty registry has zero tools, and existing `IyonCore` startup/config events remain unchanged.
- Run `cargo fmt --all -- --check`, `cargo test -p iyon-core kernel_construction`, `cargo test -p iyon-core handle`, `cargo test -p iyon-core runtime`, and `cargo check --workspace`.

**Must not:**

- Do not expose a hidden built-in/default registry on the new kernel path, move providers into the registry, or turn `iyon-native` into a second application layer.
- Do not change `iyon-api`, `iyon-tui`, `Cargo.toml`, or dependency versions for this extraction.

### Commit 5 — Add distinct native prompt, steering, and follow-up queues

**Why:** The current runtime overloads `CoreCommand::SubmitTurn` and a single steering queue to represent new work, interruption, and continuation. Later TS agents need deterministic queue semantics without losing ordering or cancellation correctness.

**Files:**

- `crates/iyon-core/src/kernel/queue.rs` (new)
- `crates/iyon-core/src/kernel/kernel.rs`
- `crates/iyon-core/src/kernel/mod.rs`
- `crates/iyon-core/src/lib.rs`
- `crates/iyon-core/src/command.rs`
- `crates/iyon-core/src/event.rs`
- `crates/iyon-core/src/runtime.rs`

**Work:**

- Add explicit native queue types and operations for `prompt`, `steer`, `follow_up`, and `abort`. Keep steering delivery at the next safe turn boundary, follow-ups ordered until the active run settles, and abort cancellation-aware with the defined partial transcript semantics.
- Use separate queue storage and APIs so a follow-up can never be mistaken for an in-run steer. Preserve FIFO order within each queue and define the cross-queue precedence in the public kernel contract; do not make queue behavior depend on TUI presentation.
- Add a distinct kernel command/input enum or methods for the new operations. Retain `CoreCommand::SubmitTurn { text }` as a compatibility wrapper whose runtime mapping preserves current behavior: steer only while the active turn is genuinely steerable, otherwise settle/cancel the old turn and start a prompt while draining pending steers in order.
- Keep command and event channels bounded. Ensure queue locks are held only for short enqueue/drain operations and never across provider/tool awaits. Make abort wake both model-turn and pending-approval waits.
- Add additive queue events for queued follow-ups/aborts only if required by the native contract; preserve existing `SteerQueued` and all current event order observed by the TUI.

**Tests / verification:**

- Add tests named `prompt_starts_a_run`, `steer_waits_for_the_next_safe_boundary`, `follow_up_waits_until_active_run_settles`, `steer_and_follow_up_keep_distinct_order`, `abort_preserves_partial_transcript`, `abort_cancels_pending_approval`, `submit_turn_compatibility_maps_to_prompt_or_steer`, and `interrupt_restart_drains_queued_steers_into_continue`.
- Assert that no steer is injected mid-model-stream, follow-ups are not consumed as steers, abort does not fabricate a result, and queue behavior remains deterministic under a full bounded event channel.
- Run `cargo fmt --all -- --check`, `cargo test -p iyon-core kernel::queue`, `cargo test -p iyon-core runtime::tests::interrupt_restart_drains_queued_steers_into_continue`, and `cargo check --workspace`.

**Must not:**

- Do not delete or rename `SubmitTurn`, turn it into the permanent new semantic API, or add default-agent continuation logic to the queue.
- Do not use a global/unbounded queue, a TUI-owned queue, or a queue callback into JavaScript.

### Commit 6 — Refactor the compatibility driver onto kernel primitives

**Why:** The new primitives are only useful if the existing Rust application exercises them. This commit makes `IyonCore` a compatibility driver over canonical sessions, model-turn normalization, lifecycle/approval state, and explicit queues while preserving current Rust-facing behavior.

**Files:**

- `crates/iyon-core/src/runtime.rs`
- `crates/iyon-core/src/agent/loop.rs`
- `crates/iyon-core/src/agent/turn.rs`
- `crates/iyon-core/src/agent/tool_execution.rs`
- `crates/iyon-core/src/agent/request.rs`
- `crates/iyon-core/src/agent/state.rs`
- `crates/iyon-core/src/agent/control.rs`
- `crates/iyon-core/src/session/state.rs`
- `crates/iyon-core/src/handle.rs`
- `crates/iyon-core/src/event.rs`

**Work:**

- Replace direct mutation of `SessionState.messages`/ad-hoc runtime counters in the compatibility path with `KernelSession` append/query/ID operations. Keep the compatibility session view synchronized through one explicit boundary so partial assistant replies, steered user messages, and tool-result messages are committed once and in order.
- Make `run_agent_loop` the legacy continuation driver: it builds requests using the compatibility request adapter, invokes the provider through the `run_model_turn` adapter, and drives the new model-turn result into the new tool lifecycle. The kernel primitives themselves remain unaware of this continuation policy.
- Wrap the current `ToolExecutor`/hook/workspace implementation around lifecycle transitions. Extract policy-free result construction and transition/event mapping; keep before/after hook ordering and current error-result behavior. Approval waits must resolve the native approval handle and use validated transitions rather than directly toggling an ad-hoc enum.
- Keep the current special `sudo` approval behavior only in an isolated compatibility/default-tool policy adapter, outside `Kernel`/`ToolLifecycle`; mark it as temporary T8-owned behavior. The new primitive accepts the resulting approval requirement without inspecting the command.
- Route old `SubmitTurn`, cancel, approve, reject, cycle-reasoning, and shutdown commands through the compatibility mapping. Keep existing `CoreEvent` variants and their order, including initial `ConfigChanged`, user-message events, partial cancellation, tool draft visibility before execution, approval resolution, and shutdown behavior.
- Retain the current 16-tool check only in this legacy/default-agent loop and label it as compatibility/default-agent policy. If the native kernel has a ceiling, make it an independently configurable host safety limit larger than product policy.
- Keep `ToolRegistry::register_builtin_defaults` reachable only from the explicit compatibility construction path. Do not add any new built-in registration to the kernel or to the future TS-facing constructor.

**Tests / verification:**

- Run the complete existing core/unit behavior set, including `thinking_deltas_are_streamed_as_core_events_in_order`, `tool_call_start_is_streamed_before_done_without_execution_start`, `tool_call_argument_fragments_preserve_order_before_end`, `cancellation_after_tool_start_preserves_partial_message`, `cancel_returns_partials_for_commit`, `steer_waits_for_response_to_complete_then_continues`, `multiple_steers_are_drained_at_once_into_one_response`, `before_hook_can_block_tool_execution`, `after_hook_can_patch_tool_result`, and `before_hooks_patch_args_in_deterministic_chain_order`.
- Run the existing public app oracle in `crates/iyon/tests/public_app.rs`, especially `streamed_tool_draft_is_visible_before_execution_and_reuses_one_card`, `prepared_tools_keep_order_and_only_started_tool_runs`, `approval_updates_prepared_tool_card_in_place`, `cancelling_preparing_tool_freezes_cancelled_card`, `cancelling_running_tool_does_not_mark_it_finished`, and `error_result_marks_prepared_card_failed_without_orphan_result_row`.
- Commands: `cargo fmt --all -- --check`, `cargo test -p iyon-core`, `cargo test -p iyon --test public_app`, and `cargo check --workspace`.

**Must not:**

- Do not migrate the default agent, providers, or built-in tools to TypeScript; this is only the compatibility driver refactor required to keep them working.
- Do not change `iyon-tui` or its rendering assumptions, remove public Rust APIs, or let compatibility-only policy leak into the new kernel types.

### Commit 7 — Add direct kernel integration proof and publish the Rust surface

**Why:** The tranche exit criterion is a direct native proof of the primitive chain, plus an accurate public contract for T4. The proof must not pass through the current hard-coded agent loop.

**Files:**

- `crates/iyon-core/src/kernel/tests.rs` (new, or the corresponding `#[cfg(test)]` kernel module file)
- `crates/iyon-core/src/kernel/mod.rs`
- `crates/iyon-core/src/lib.rs`
- `iyon-core.md`
- `tools/api-surface/**` (only the T2-established kernel inventory/baseline files)

**Work:**

- Add one end-to-end Rust test named `manual_kernel_session_model_turn_tool_lifecycle_transcript_flow`. It must create an injected empty kernel/session, append a user message, drive a `ModelTurn` with manually supplied provider events, obtain one assembled tool call, transition a native tool lifecycle through preparation and approval to running and finished using a test result, append the assistant/tool-result messages to the canonical session, and assert the final snapshot and normalized event sequence. It must not construct `IyonCore`, call `run_agent_loop`, call `register_builtin_defaults`, or invoke a real provider/tool.
- Add focused contract assertions for custom entries surviving the flow, native IDs remaining stable, a cancelled model turn retaining its partial assistant entry, a rejected approval not producing a success result, and a second append/query operation observing the same canonical transcript without model-request mutation.
- Update `iyon-core.md` and the T2 API-surface records with every newly reachable public kernel item, exact module/root re-export paths, transition/error/event semantics, and an explicit compatibility disposition for retained legacy APIs. Keep the inventory’s missing and stale counts at zero.
- Record the public boundary T4 should bind: value types and snapshots versus stateful native handles, explicit cancellation ownership, bounded event consumption, and the fact that provider events are pushed into Rust rather than Rust calling back into JS.
- Run the full Rust workspace verification and the API-surface checker established by T2. There is no existing tracked CI workflow in this repository; do not invent one in T3. If T2 has added a required CI command, run that command locally and keep this commit limited to the established inventory files.

**Tests / verification:**

- Assert the direct test covers `session → model turn → assembled tool call → tool lifecycle → transcript result` without the compatibility loop.
- Run `test -f iyon-core.md`, `cargo fmt --all -- --check`, `cargo test --workspace`, `cargo check --workspace`, and the T2 API-surface command that verifies `missing=0 stale=0`.
- Review the final dependency graph to confirm `iyon-core` still depends only on `iyon-api` among Iyon crates, `iyon-tui` remains independent, and no N-API/TS files were added.

**Must not:**

- Do not add N-API exports, TypeScript fixtures, provider/tool migrations, TUI edits, architecture rewrites, or a CI workflow.
- Do not mark later-tranche policy as part of the kernel contract merely because the direct test needs a neutral approval requirement or a manually supplied tool result.

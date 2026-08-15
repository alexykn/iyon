# `iyon-core` public API

This inventory follows the public items reachable from `crates/iyon-core/src/lib.rs`.
Items in `pub(crate)` modules are omitted unless a public re-export makes them
reachable. Signatures use the source-level types; `Result` in this document means
`anyhow::Result` where the source imports it unqualified.

## Crate root: `iyon_core`

### Public modules

| Path | Kind | Purpose |
| --- | --- | --- |
| `iyon_core::ids` | module (definition) | Public identifier newtypes used by core sessions, turns, messages, tools, and approvals. |
| `iyon_core::tools` | module (definition) | Public tool extension APIs, including hooks and output truncation helpers. |

The following root names are re-exports. Their definitions and members are
described in the sections below.

| Path | Kind | Source / purpose |
| --- | --- | --- |
| `iyon_core::CoreCommand` | enum (re-export) | Re-export of the command enum defined in the private `command` module; commands sent to the core. |
| `iyon_core::CoreEvent` | enum (re-export) | Re-export of the event enum defined in the private `event` module; events emitted by the core. |
| `iyon_core::MessageDelta` | enum (re-export) | Re-export of the message-stream delta enum from the private `event` module. |
| `iyon_core::MessageRole` | enum (re-export) | Re-export of the message-role enum from the private `event` module. |
| `iyon_core::ToolCallDelta` | enum (re-export) | Re-export of streamed tool-call deltas from the private `event` module. |
| `iyon_core::ToolUpdateEvent` | enum (re-export) | Re-export of tool progress/detail updates from the private `event` module. |
| `iyon_core::IyonCore` | struct (re-export) | Re-export of the core handle defined in the private `handle` module. |
| `iyon_core::CoreCommandSender` | struct (re-export) | Re-export of the command sender defined in the private `handle` module. |
| `iyon_core::CoreEventReceiver` | struct (re-export) | Re-export of the event receiver defined in the private `handle` module. |
| `iyon_core::ApprovalId` | struct (re-export) | Re-export of `iyon_core::ids::ApprovalId`. |
| `iyon_core::MessageId` | struct (re-export) | Re-export of `iyon_core::ids::MessageId`. |
| `iyon_core::SessionId` | struct (re-export) | Re-export of `iyon_core::ids::SessionId`. |
| `iyon_core::ToolCallId` | struct (re-export) | Re-export of `iyon_core::ids::ToolCallId`. |
| `iyon_core::TurnId` | struct (re-export) | Re-export of `iyon_core::ids::TurnId`. |
| `iyon_core::ReasoningLevel` | enum (re-export from `iyon-api`) | Re-export of the provider reasoning-level type from `iyon_api`; used in session configuration and configuration-change events. |
| `iyon_core::ModelSelection` | struct (re-export) | Re-export of the model-selection type defined in the private `session::state` module. |
| `iyon_core::SessionState` | struct (re-export) | Re-export of session state defined in the private `session::state` module. |

### Free functions

| Path | Kind | Signature | Purpose |
| --- | --- | --- | --- |
| `iyon_core::spawn` | free function (definition) | `pub fn spawn() -> anyhow::Result<IyonCore>` | Creates a core using the default mock model and an owned Tokio runtime. |

## Commands and events

### `iyon_core::CoreCommand`

Definition in the private `command` module; public at the root through re-export.

| Variant | Fields | Purpose |
| --- | --- | --- |
| `SubmitTurn` | `text: String` | Submit a user turn, or steer the active turn when it can accept steering. |
| `CancelActiveTurn` | — | Request cancellation of the active turn. |
| `CycleReasoningEffort` | — | Advance the session reasoning effort to the next level supported by the current provider. |
| `ApproveToolCall` | `approval_id: u64` | Approve a pending tool call. |
| `RejectToolCall` | `approval_id: u64`, `reason: Option<String>` | Reject a pending tool call with an optional reason. |
| `Shutdown` | — | Stop the core runtime and active turn. |

### `iyon_core::CoreEvent`

Definition in the private `event` module; public at the root through re-export.

| Variant | Fields | Purpose |
| --- | --- | --- |
| `AgentStarted` | — | Signals that agent processing has started. |
| `AgentFinished` | — | Signals that agent processing has finished. |
| `TurnStarted` | `turn_id: u64` | Signals the start of a turn. |
| `SteerQueued` | `text: String` | Reports a message queued for delivery at the next turn boundary. |
| `MessageStarted` | `turn_id: u64`, `message_id: u64`, `role: MessageRole` | Signals the start of a streamed message. |
| `MessageDelta` | `turn_id: u64`, `message_id: u64`, `delta: MessageDelta` | Carries an incremental message update. |
| `MessageFinished` | `turn_id: u64`, `message_id: u64` | Signals that a message is complete. |
| `ToolCallStarted` | `turn_id: u64`, `message_id: u64`, `tool_call_id: String`, `tool_name: String`, `arguments: serde_json::Value` | Signals the start of a tool call. |
| `ToolCallFinished` | `turn_id: u64`, `message_id: u64`, `tool_call_id: String`, `tool_name: String`, `is_error: bool` | Signals completion of a tool call. |
| `ToolCallUpdated` | `turn_id: u64`, `message_id: u64`, `tool_call_id: String`, `tool_name: String`, `update: ToolUpdateEvent` | Carries an update emitted while a tool call is running. |
| `ToolResultStarted` | `turn_id: u64`, `message_id: u64`, `tool_call_id: String`, `tool_name: String`, `is_error: bool` | Signals the start of tool-result delivery. |
| `ToolResultFinished` | `turn_id: u64`, `message_id: u64`, `tool_call_id: String`, `tool_name: String`, `text: String`, `details: serde_json::Value`, `is_error: bool` | Carries the final rendered result of a tool call. |
| `ToolApprovalRequested` | `turn_id: u64`, `approval_id: u64`, `message_id: u64`, `tool_call_id: String`, `tool_name: String`, `arguments: serde_json::Value` | Requests approval before executing a tool call. |
| `ToolApprovalResolved` | `turn_id: u64`, `approval_id: u64`, `tool_call_id: String`, `approved: bool`, `reason: Option<String>` | Reports the decision for a tool approval request. |
| `TurnFinished` | `turn_id: u64` | Signals successful turn completion. |
| `TurnFailed` | `turn_id: u64`, `message: String` | Reports turn failure. |
| `TurnCancelled` | `turn_id: u64` | Reports turn cancellation. |
| `ConfigChanged` | `provider: String`, `model_id: String`, `reasoning_effort: ReasoningLevel` | Reports the current provider, model, and reasoning effort, including at startup. |

### `iyon_core::MessageRole`

Definition in the private `event` module; public at the root through re-export.

| Variant | Purpose |
| --- | --- |
| `User` | A user-authored message. |
| `Assistant` | An assistant-authored message. |
| `ToolResult` | A tool result represented as a message. |
| `Status` | A status or informational message. |

### `iyon_core::ToolUpdateEvent`

Definition in the private `event` module; public at the root through re-export.

| Variant | Fields | Purpose |
| --- | --- | --- |
| `Text` | `String` | Reports textual tool output or status. |
| `Progress` | `label: String`, `current: Option<u64>`, `total: Option<u64>` | Reports progress, optionally with a current and total count. |
| `Details` | `serde_json::Value` | Reports structured tool details. |

### `iyon_core::MessageDelta`

Definition in the private `event` module; public at the root through re-export.

| Variant | Fields | Purpose |
| --- | --- | --- |
| `Text` | `String` | Incremental assistant text. |
| `Thinking` | `String` | Incremental reasoning/thinking text. |
| `ToolCall` | `ToolCallDelta` | Incremental tool-call content. |

### `iyon_core::ToolCallDelta`

Definition in the private `event` module; public at the root through re-export.

| Variant | Fields | Purpose |
| --- | --- | --- |
| `Start` | `content_index: usize`, `tool_call_id: Option<String>`, `tool_name: Option<String>` | Begins a streamed tool call. |
| `Arguments` | `content_index: usize`, `tool_call_id: Option<String>`, `tool_name: Option<String>`, `delta: String` | Adds an argument-text delta to a streamed tool call. |
| `End` | `content_index: usize`, `tool_call_id: String`, `tool_name: String`, `arguments: serde_json::Value` | Completes a streamed tool call with assembled arguments. |

## Core handles

### `iyon_core::IyonCore`

Definition in the private `handle` module; public at the root through re-export.
It owns or borrows the runtime needed to process commands and events, depending on
the constructor used.

| Path | Kind | Signature | Purpose |
| --- | --- | --- | --- |
| `IyonCore::spawn` | inherent method | `pub fn spawn() -> anyhow::Result<Self>` | Creates a core with the default mock model and an owned runtime. |
| `IyonCore::spawn_with_owned_runtime` | inherent method | `pub fn spawn_with_owned_runtime(model: Arc<dyn iyon_api::ModelApi>) -> anyhow::Result<Self>` | Creates a core with a caller-supplied model provider and an owned Tokio runtime. |
| `IyonCore::spawn_with_owned_runtime_and_hooks` | inherent method | `pub fn spawn_with_owned_runtime_and_hooks(model: Arc<dyn iyon_api::ModelApi>, tool_hooks: ToolHookSet) -> anyhow::Result<Self>` | Creates a core with a supplied model provider, tool hooks, and an owned runtime. |
| `IyonCore::spawn_default_on_current_runtime` | inherent method | `#[must_use] pub fn spawn_default_on_current_runtime() -> Self` | Creates a core with the default mock model on the caller’s current Tokio runtime. |
| `IyonCore::spawn_on_current_runtime` | inherent method | `pub fn spawn_on_current_runtime(model: Arc<dyn iyon_api::ModelApi>) -> Self` | Creates a core with a supplied model provider on the caller’s current Tokio runtime. |
| `IyonCore::spawn_on_current_runtime_and_hooks` | inherent method | `pub fn spawn_on_current_runtime_and_hooks(model: Arc<dyn iyon_api::ModelApi>, tool_hooks: ToolHookSet) -> Self` | Creates a core with a supplied model provider and hooks on the caller’s current runtime. |
| `IyonCore::spawn_on_current_runtime_with_selection_and_hooks` | inherent method | `pub fn spawn_on_current_runtime_with_selection_and_hooks(model: Arc<dyn iyon_api::ModelApi>, selection: ModelSelection, tool_hooks: ToolHookSet) -> Self` | Creates a core on the current runtime with an explicit model selection and hooks. |
| `IyonCore::split` | inherent method | `pub fn split(self) -> (CoreCommandSender, CoreEventReceiver)` | Splits the core into independently usable command-sender and event-receiver handles. |
| `IyonCore::try_send` | inherent method | `pub fn try_send(&self, command: CoreCommand) -> anyhow::Result<()>` | Tries to enqueue a command without awaiting channel capacity. |
| `IyonCore::try_recv_event` | inherent method | `pub fn try_recv_event(&mut self) -> Option<CoreEvent>` | Tries to receive one event without blocking. |
| `IyonCore::blocking_recv_event` | inherent method | `pub fn blocking_recv_event(&mut self) -> Option<CoreEvent>` | Receives one event while blocking the current synchronous thread. |
| `IyonCore::drain_events` | inherent method | `pub fn drain_events(&mut self) -> Vec<CoreEvent>` | Drains all currently available events. |

### `iyon_core::CoreCommandSender`

Definition in the private `handle` module; public at the root through re-export.

| Path | Kind | Signature | Purpose |
| --- | --- | --- | --- |
| `CoreCommandSender::try_send` | inherent method | `pub fn try_send(&self, command: CoreCommand) -> anyhow::Result<()>` | Tries to enqueue a command without awaiting channel capacity. |

### `iyon_core::CoreEventReceiver`

Definition in the private `handle` module; public at the root through re-export.

| Path | Kind | Signature | Purpose |
| --- | --- | --- | --- |
| `CoreEventReceiver::recv_event` | inherent method | `pub async fn recv_event(&mut self) -> Option<CoreEvent>` | Asynchronously receives the next core event. |
| `CoreEventReceiver::try_recv_event` | inherent method | `pub fn try_recv_event(&mut self) -> Option<CoreEvent>` | Tries to receive one event without blocking. |
| `CoreEventReceiver::blocking_recv_event` | inherent method | `pub fn blocking_recv_event(&mut self) -> Option<CoreEvent>` | Receives one event while blocking the current synchronous thread. |
| `CoreEventReceiver::drain_events` | inherent method | `pub fn drain_events(&mut self) -> Vec<CoreEvent>` | Drains all currently available events. |

## `iyon_core::ids`

All five identifier types below are definitions in the public `ids` module and
are also re-exported at the crate root.

| Path | Kind | Fields | Purpose |
| --- | --- | --- | --- |
| `iyon_core::ids::SessionId` | tuple struct (definition; root re-export) | `pub .0: u64` | Identifies a session. |
| `iyon_core::ids::TurnId` | tuple struct (definition; root re-export) | `pub .0: u64` | Identifies a turn. |
| `iyon_core::ids::MessageId` | tuple struct (definition; root re-export) | `pub .0: u64` | Identifies a message. |
| `iyon_core::ids::ToolCallId` | tuple struct (definition; root re-export) | `pub .0: String` | Identifies a tool call. |
| `iyon_core::ids::ApprovalId` | tuple struct (definition; root re-export) | `pub .0: u64` | Identifies a tool approval request. |

No inherent public methods are defined on these identifier types.

## Session configuration and state

### `iyon_core::ModelSelection`

Definition in the private `session::state` module; public at the root through
re-export. It is also the selection argument accepted by
`IyonCore::spawn_on_current_runtime_with_selection_and_hooks`.

| Field | Type | Purpose |
| --- | --- | --- |
| `ModelSelection::provider` | `String` | Provider name used for the session. |
| `ModelSelection::model_id` | `String` | Model identifier used for the session. |

No inherent public methods are defined on `ModelSelection`.

### `iyon_core::SessionState`

Definition in the private `session::state` module; public at the root through
re-export.

| Path | Kind | Signature / field | Purpose |
| --- | --- | --- | --- |
| `SessionState::id` | public field | `SessionId` | Session identifier. |
| `SessionState::cwd` | public field | `PathBuf` | Working directory for the session. |
| `SessionState::messages` | public field | `Vec<AgentMessage>` | Messages accumulated by the session. |
| `SessionState::model` | public field | `ModelSelection` | Selected provider and model. |
| `SessionState::reasoning_effort` | public field | `ReasoningLevel` | Current reasoning effort. |
| `SessionState::system_prompt` | public field | `String` | Session system prompt. |
| `SessionState::metadata` | public field | `SessionMetadata` | Session metadata, including the optional user identifier. |
| `SessionState::new` | inherent method | `pub fn new(id: SessionId, cwd: PathBuf) -> Self` | Constructs session state with default mock model selection and medium reasoning effort. |

`AgentMessage` and `SessionMetadata` are types in private modules and are not
separately reachable from the crate root; they are shown only because they are
the declared types of public `SessionState` fields.

## `iyon_core::tools`

The `tools` module is public. Its `hooks` and `output` submodules are public;
the `builtin`, `definition`, `executor`, `file_mutation_queue`, `process`, and
`registry` submodules are `pub(crate)` and are not part of this API inventory.

The following names are public re-exports from `iyon_core::tools::hooks`:

| Path | Kind | Purpose |
| --- | --- | --- |
| `iyon_core::tools::AfterToolCallContext` | struct (re-export) | Context passed to an after-tool-call hook. |
| `iyon_core::tools::AfterToolCallHook` | trait (re-export) | Hook contract for observing or patching completed tool calls. |
| `iyon_core::tools::AfterToolCallPatch` | struct (re-export) | Optional changes an after hook can apply to a tool result. |
| `iyon_core::tools::BeforeToolCallContext` | struct (re-export) | Context passed to a before-tool-call hook. |
| `iyon_core::tools::BeforeToolCallDecision` | enum (re-export) | Decision returned by a before hook. |
| `iyon_core::tools::BeforeToolCallHook` | trait (re-export) | Hook contract for allowing, patching, or blocking a tool call. |
| `iyon_core::tools::BeforeToolCallResolution` | enum (re-export) | Combined result after all before hooks run. |
| `iyon_core::tools::ToolHookSet` | struct (re-export) | Mutable collection of before and after hooks. |
| `iyon_core::tools::ToolHookSnapshot` | struct (re-export) | Cloned hook collection used while executing a turn. |

The definitions and all members of those re-exported items follow.

### `iyon_core::tools::hooks`

#### Type aliases

| Path | Kind | Signature | Purpose |
| --- | --- | --- | --- |
| `iyon_core::tools::hooks::BeforeHookFuture<'a>` | type alias (definition) | `Pin<Box<dyn Future<Output = anyhow::Result<BeforeToolCallDecision>> + Send + 'a>>` | Future returned by a before-tool-call hook. |
| `iyon_core::tools::hooks::AfterHookFuture<'a>` | type alias (definition) | `Pin<Box<dyn Future<Output = anyhow::Result<Option<AfterToolCallPatch>>> + Send + 'a>>` | Future returned by an after-tool-call hook. |

#### Traits

| Path | Kind | Purpose |
| --- | --- | --- |
| `iyon_core::tools::hooks::BeforeToolCallHook` | trait (definition; re-exported at `iyon_core::tools::BeforeToolCallHook`) | Allows a hook to inspect and allow, patch, or block a tool call before execution. |
| `iyon_core::tools::hooks::AfterToolCallHook` | trait (definition; re-exported at `iyon_core::tools::AfterToolCallHook`) | Allows a hook to inspect a completed tool call and optionally patch its result. |

| Trait method | Signature | Purpose |
| --- | --- | --- |
| `BeforeToolCallHook::before_tool_call` | `fn before_tool_call<'a>(&'a self, ctx: BeforeToolCallContext<'a>) -> BeforeHookFuture<'a>` | Runs before tool execution and returns the hook’s decision asynchronously. |
| `AfterToolCallHook::after_tool_call` | `fn after_tool_call<'a>(&'a self, ctx: AfterToolCallContext<'a>) -> AfterHookFuture<'a>` | Runs after tool execution and returns an optional result patch asynchronously. |

#### `BeforeToolCallContext<'a>`

Kind: struct (definition; re-exported at `iyon_core::tools::BeforeToolCallContext`).

Definition at `iyon_core::tools::hooks::BeforeToolCallContext`; also re-exported
at `iyon_core::tools::BeforeToolCallContext`.

| Field | Type | Purpose |
| --- | --- | --- |
| `turn_id` | `TurnId` | Turn containing the tool call. |
| `tool_call_id` | `&'a ToolCallId` | Tool-call identifier. |
| `tool_name` | `&'a str` | Registered tool name. |
| `args` | `&'a serde_json::Value` | Current tool arguments. |
| `session` | `&'a SessionState` | Session state visible to the hook. |

#### `BeforeToolCallDecision`

Kind: enum (definition; re-exported at `iyon_core::tools::BeforeToolCallDecision`).

Definition at `iyon_core::tools::hooks::BeforeToolCallDecision`; also re-exported
at `iyon_core::tools::BeforeToolCallDecision`.

| Variant | Fields | Purpose |
| --- | --- | --- |
| `Allow` | — | Allow the call unchanged. |
| `PatchArgs` | `serde_json::Value` | Replace the current arguments before continuing. |
| `Block` | `reason: Option<String>` | Prevent execution, optionally with a reason. |

#### `BeforeToolCallResolution`

Kind: enum (definition; re-exported at `iyon_core::tools::BeforeToolCallResolution`).

Definition at `iyon_core::tools::hooks::BeforeToolCallResolution`; also
re-exported at `iyon_core::tools::BeforeToolCallResolution`.

| Variant | Fields | Purpose |
| --- | --- | --- |
| `Proceed` | `args: serde_json::Value` | Continue execution with the final arguments. |
| `Block` | `reason: Option<String>` | Stop execution, optionally with a reason. |

#### `AfterToolCallContext<'a>`

Kind: struct (definition; re-exported at `iyon_core::tools::AfterToolCallContext`).

Definition at `iyon_core::tools::hooks::AfterToolCallContext`; also re-exported
at `iyon_core::tools::AfterToolCallContext`.

| Field | Type | Purpose |
| --- | --- | --- |
| `turn_id` | `TurnId` | Turn containing the tool call. |
| `tool_call_id` | `&'a ToolCallId` | Tool-call identifier. |
| `tool_name` | `&'a str` | Registered tool name. |
| `args` | `&'a serde_json::Value` | Arguments used for the call. |
| `result` | `&'a ToolResult` | Tool result before after-hook patches are merged. |
| `session` | `&'a SessionState` | Session state visible to the hook. |

`ToolResult` is defined in the private `tools::executor` module and is not
separately reachable from the crate root; it is shown only as the declared type
of this public context field.

#### `AfterToolCallPatch`

Kind: struct (definition; re-exported at `iyon_core::tools::AfterToolCallPatch`).

Definition at `iyon_core::tools::hooks::AfterToolCallPatch`; also re-exported
at `iyon_core::tools::AfterToolCallPatch`.

| Field | Type | Purpose |
| --- | --- | --- |
| `content_override` | `Option<Vec<iyon_api::ContentBlock>>` | Optional replacement tool content. |
| `details_override` | `Option<serde_json::Value>` | Optional replacement structured details. |
| `is_error_override` | `Option<bool>` | Optional replacement error flag. |
| `terminate_override` | `Option<bool>` | Optional replacement termination flag. |

#### `ToolHookSet`

Kind: struct (definition; re-exported at `iyon_core::tools::ToolHookSet`).

Definition at `iyon_core::tools::hooks::ToolHookSet`; also re-exported at
`iyon_core::tools::ToolHookSet`.

| Path | Kind | Signature | Purpose |
| --- | --- | --- | --- |
| `ToolHookSet::register_before` | inherent method | `pub fn register_before(&mut self, hook: Arc<dyn BeforeToolCallHook>)` | Registers a before-tool-call hook. |
| `ToolHookSet::register_before_fn` | inherent method | `pub fn register_before_fn<F>(&mut self, hook: F) where F: BeforeToolCallHook + 'static` | Wraps and registers an owned before-tool-call hook value. |
| `ToolHookSet::register_after` | inherent method | `pub fn register_after(&mut self, hook: Arc<dyn AfterToolCallHook>)` | Registers an after-tool-call hook. |
| `ToolHookSet::register_after_fn` | inherent method | `pub fn register_after_fn<F>(&mut self, hook: F) where F: AfterToolCallHook + 'static` | Wraps and registers an owned after-tool-call hook value. |
| `ToolHookSet::snapshot` | inherent method | `#[must_use] pub fn snapshot(&self) -> ToolHookSnapshot` | Clones the registered hooks into an execution snapshot. |

#### `ToolHookSnapshot`

Kind: struct (definition; re-exported at `iyon_core::tools::ToolHookSnapshot`).

Definition at `iyon_core::tools::hooks::ToolHookSnapshot`; also re-exported at
`iyon_core::tools::ToolHookSnapshot`.

| Path | Kind | Signature | Purpose |
| --- | --- | --- | --- |
| `ToolHookSnapshot::run_before_hooks` | inherent method | `pub async fn run_before_hooks(&self, ctx: BeforeToolCallContext<'_>) -> anyhow::Result<BeforeToolCallResolution>` | Runs before hooks in registration order and returns the final decision. |
| `ToolHookSnapshot::run_after_hooks` | inherent method | `pub async fn run_after_hooks(&self, ctx: AfterToolCallContext<'_>) -> anyhow::Result<AfterToolCallPatch>` | Runs after hooks in registration order and merges their patches. |

## `iyon_core::tools::output`

### Constants

| Path | Kind | Signature | Purpose |
| --- | --- | --- | --- |
| `iyon_core::tools::output::DEFAULT_MODEL_MAX_LINES` | const | `pub const ...: usize = 2_000` | Default maximum number of model-output lines. |
| `iyon_core::tools::output::DEFAULT_MODEL_MAX_BYTES` | const | `pub const ...: usize = 50 * 1024` | Default maximum number of model-output bytes. |
| `iyon_core::tools::output::GREP_MAX_LINE_CHARS` | const | `pub const ...: usize = 500` | Maximum characters retained for a grep output line. |

### `iyon_core::tools::output::TruncationStrategy`

Kind: enum (definition).

| Variant | Purpose |
| --- | --- |
| `Head` | Retain output from the beginning. |
| `Tail` | Retain output from the end. |
| `Line` | Truncate an individual line. |

### `iyon_core::tools::output::TruncatedBy`

Kind: enum (definition).

| Variant | Purpose |
| --- | --- |
| `Lines` | Indicates a line-count limit caused truncation. |
| `Bytes` | Indicates a byte-count limit caused truncation. |
| `Characters` | Indicates a character-count limit caused truncation. |

### `iyon_core::tools::output::TruncationReport`

Kind: struct (definition).

| Field | Type | Purpose |
| --- | --- | --- |
| `scope` | `&'static str` | Output domain being reported. |
| `strategy` | `TruncationStrategy` | Strategy used to truncate. |
| `truncated` | `bool` | Whether the content was truncated. |
| `truncated_by` | `Option<TruncatedBy>` | Limit responsible for truncation, if any. |
| `total_lines` | `usize` | Number of input lines. |
| `output_lines` | `usize` | Number of output lines. |
| `total_bytes` | `usize` | Number of input bytes. |
| `output_bytes` | `usize` | Number of output bytes. |
| `max_lines` | `Option<usize>` | Applied line limit. |
| `max_bytes` | `Option<usize>` | Applied byte limit. |
| `first_line_exceeds_limit` | `bool` | Whether the first line itself exceeded the byte limit. |
| `last_line_partial` | `bool` | Whether the retained final line is partial. |

### `iyon_core::tools::output::TruncatedText`

Kind: struct (definition).

| Field | Type | Purpose |
| --- | --- | --- |
| `text` | `String` | Resulting text after truncation. |
| `report` | `TruncationReport` | Details of the truncation operation. |

### `iyon_core::tools::output::ModelOutputLimits`

Kind: struct (definition).

| Field | Type | Purpose |
| --- | --- | --- |
| `max_lines` | `usize` | Maximum output lines. |
| `max_bytes` | `usize` | Maximum output bytes. |

### Free functions

| Path | Kind | Signature | Purpose |
| --- | --- | --- | --- |
| `iyon_core::tools::output::truncate_head` | free function | `pub fn truncate_head(content: &str, limits: ModelOutputLimits) -> TruncatedText` | Retains the beginning of content within line and byte limits. |
| `iyon_core::tools::output::truncate_tail` | free function | `pub fn truncate_tail(content: &str, limits: ModelOutputLimits) -> TruncatedText` | Retains the end of content within line and byte limits. |
| `iyon_core::tools::output::truncate_line` | free function | `pub fn truncate_line(line: &str, max_chars: usize) -> (String, bool)` | Truncates one line by character count and reports whether it changed. |
| `iyon_core::tools::output::truncation_details` | free function | `pub fn truncation_details(report: &TruncationReport) -> serde_json::Value` | Encodes a truncation report as JSON details. |

## Provider usage

The core accepts a model through `Arc<dyn iyon_api::ModelApi>` in its spawning
constructors. `IyonCore::spawn_with_owned_runtime` and
`IyonCore::spawn_on_current_runtime` accept a provider directly; their
`*_and_hooks` variants additionally accept a `ToolHookSet`, and
`spawn_on_current_runtime_with_selection_and_hooks` also accepts a
`ModelSelection`. The convenience `spawn` constructors construct and use
`iyon_api::MockModelApi`. Provider discovery or environment-based selection is
outside `iyon-core`.

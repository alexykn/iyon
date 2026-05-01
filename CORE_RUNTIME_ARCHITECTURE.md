# Core Runtime Architecture

This document describes the target backend/core architecture for `iyon`.

It focuses on clear API boundaries, extensibility, agent-loop state management, tool execution, and how the TUI talks to the core runtime.

## Goals

- Keep the TUI replaceable.
- Keep model providers replaceable.
- Keep tools and extensions pluggable.
- Keep the core runtime responsible for orchestration and state.
- Keep provider APIs UI-agnostic and tool-execution-agnostic.
- Use simple, typed, easy-to-consume public APIs.

## Crate Boundaries

```txt
iyon-tui  ->  iyon-core  ->  iyon-api
```

### `iyon-api`

Provider-neutral model API layer.

Owns:
- generic model request/response types
- model message/content types
- provider-normalized stream events
- model/provider trait
- provider implementations

Does not own:
- agent loop
- session state
- tool execution
- filesystem operations
- TUI concepts
- mpsc frontend/backend channels

### `iyon-core`

Product runtime and backend orchestration layer.

Owns:
- public runtime handle used by clients
- mpsc command/event boundary
- agent loop
- session state
- transcript state
- request construction
- tool registry and execution
- filesystem safety layer
- extension/plugin registry and hooks
- cancellation and approval flow

Auth/session bootstrap note:
- OAuth login, credential storage, and token refresh happen in the app/bootstrap layer (`iyon` binary), not in `iyon-core`.
- `iyon-core` receives an already-constructed `Arc<dyn ModelApi>` and remains provider/auth agnostic.

### `iyon-tui`

One client of `iyon-core`.

Owns:
- terminal input
- render state
- active pane state
- transcript presentation
- scrollback/rendering

Does not own:
- model calls
- agent loop
- tool execution
- backend mpsc setup
- provider selection internals

The TUI sends `CoreCommand` values and drains `CoreEvent` values.

Tool hook snapshot note:
- `iyon-core` owns before/after tool hooks as orchestration policy.
- Runtime receives a `ToolHookSet` at core spawn and snapshots it per turn.
- Runtime passes an immutable per-turn `ToolHookSnapshot` into the agent loop.
- This keeps hook behavior deterministic per turn and avoids mutable global hook state races.

## Conversation vs presentation ownership

Core is the source of truth for semantic conversation state. The TUI may keep a transcript-shaped presentation cache, but that cache is a projection of `CoreEvent` messages, not an independent conversation log.

Consequences:
- user input is sent as `CoreCommand::SubmitTurn`; the TUI does not commit a user transcript item optimistically
- core appends the user `AgentMessage` to `SessionState` and emits user `MessageStarted`/`MessageDelta`/`MessageFinished` events
- assistant/tool/status messages follow the same semantic event path
- TUI-owned active panes, markdown formatting, layout, and scrollback remain presentation concerns only

This keeps message IDs, turn IDs, transcript ordering, request construction, and future persistence under core ownership while preserving a replaceable frontend.

---

## High-level Runtime Flow

```txt
TUI input
  -> iyon_core::CoreCommand
  -> runtime supervisor
  -> agent loop
  -> iyon_api::ModelApi
  -> provider stream
  -> model stream events
  -> agent state updates
  -> tool execution when needed
  -> iyon_core::CoreEvent
  -> TUI presentation/rendering
```

One submitted user turn:

```txt
SubmitTurn
  -> append user AgentMessage
  -> emit TurnStarted
  -> build ModelRequest from SessionState
  -> stream assistant response from ModelApi
  -> emit message deltas
  -> if tool calls are requested:
       validate args
       run approval policy
       execute tools
       append tool result messages
       continue model loop
  -> if no more tool calls:
       emit TurnFinished
```

---

# `iyon-api`

`iyon-api` is intentionally narrow. It should model what providers understand, not what the app or TUI needs.

## Core Types

```rust
pub struct ModelRequest {
    pub system_prompt: Option<String>,
    pub messages: Vec<ModelMessage>,
    pub tools: Vec<ModelToolSpec>,
    pub params: ModelParams,
    pub metadata: ModelMetadata,
}
```

```rust
pub struct ModelParams {
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub reasoning: Option<ReasoningLevel>,
    pub cache_retention: Option<CacheRetention>,
}
```

```rust
pub struct ModelMetadata {
    pub session_id: Option<String>,
    pub user_id: Option<String>,
}
```

```rust
pub enum ModelMessage {
    User {
        content: Vec<ContentBlock>,
    },
    Assistant {
        content: Vec<ContentBlock>,
    },
    ToolResult {
        tool_call_id: String,
        tool_name: String,
        content: Vec<ContentBlock>,
        is_error: bool,
    },
}
```

```rust
pub enum ContentBlock {
    Text {
        text: String,
    },
    Image {
        data: Vec<u8>,
        mime_type: String,
    },
    Thinking {
        text: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
}
```

## Tools as Seen by Models

```rust
pub struct ModelToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}
```

This is only the schema advertised to the model. Actual execution lives in `iyon-core`.

## Stream Events

The generic API should stream semantic events, not UI-ready strings.

```rust
pub enum ModelStreamEvent {
    Started,

    TextStart {
        content_index: usize,
    },
    TextDelta {
        content_index: usize,
        delta: String,
    },
    TextEnd {
        content_index: usize,
        text: String,
    },

    ThinkingStart {
        content_index: usize,
    },
    ThinkingDelta {
        content_index: usize,
        delta: String,
    },
    ThinkingEnd {
        content_index: usize,
        text: String,
    },

    ToolCallStart {
        content_index: usize,
        id: Option<String>,
        name: Option<String>,
    },
    ToolCallDelta {
        content_index: usize,
        id: Option<String>,
        name: Option<String>,
        arguments_delta: String,
    },
    ToolCallEnd {
        content_index: usize,
        id: String,
        name: String,
        arguments: serde_json::Value,
    },

    Usage {
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: u64,
        cache_write_tokens: u64,
    },

    Done {
        stop_reason: StopReason,
    },
    Error {
        message: String,
    },
}
```

```rust
pub enum StopReason {
    Stop,
    Length,
    ToolUse,
    Error,
    Aborted,
}
```

## Model API Trait

```rust
pub trait ModelApi: Send + Sync {
    fn stream(&self, request: ModelRequest) -> ModelStreamFuture<'_>;
}
```

`ModelStream` is a boxed async stream so `iyon-core` can hold providers behind `Arc<dyn ModelApi>` while each provider keeps its own concrete stream implementation private:

```rust
pub type ModelStream = Pin<
    Box<dyn Stream<Item = Result<ModelStreamEvent, ModelError>> + Send>
>;

pub type ModelStreamFuture<'a> = Pin<
    Box<dyn Future<Output = Result<ModelStream, ModelError>> + Send + 'a>
>;
```

The core/provider boundary is a direct async stream API, not an mpsc actor boundary. mpsc is reserved for the client/core command-event boundary. Provider implementations may use channels internally if their transport is callback-driven, but that is hidden behind `ModelStream`.

Provider failures should be normalized into `ModelStreamEvent::Error` or `ModelError` depending on whether the stream could be established.

## Suggested `iyon-api` Layout

```txt
crates/iyon-api/src/
  lib.rs
  client.rs          # ModelApi trait, ModelStream alias
  model.rs           # ModelRequest, ModelMessage, ContentBlock
  stream.rs          # ModelStreamEvent, StopReason, Usage
  tool.rs            # ModelToolSpec
  error.rs           # ModelError
  providers/
    mod.rs
    mock.rs
    openai.rs
    anthropic.rs
```

---

# `iyon-core`

`iyon-core` is the backend runtime. It owns the mpsc channels and exposes a small handle to clients.

## Public Handle

Client code should feel like this:

```rust
let mut core = iyon_core::spawn(CoreConfig::default())?;

core.try_send(CoreCommand::SubmitTurn {
    text: "hello".to_string(),
})?;

for event in core.drain_events() {
    // UI handles event
}
```

## `IyonCore`

```rust
pub struct IyonCore {
    command_tx: tokio::sync::mpsc::Sender<CoreCommand>,
    event_rx: tokio::sync::mpsc::Receiver<CoreEvent>,
}
```

```rust
impl IyonCore {
    pub fn try_send(&self, command: CoreCommand) -> Result<()>;

    pub fn drain_events(&mut self) -> Vec<CoreEvent>;

    pub async fn recv_event(&mut self) -> Option<CoreEvent>;
}
```

Internally, `spawn` creates both channels:

```rust
let (command_tx, command_rx) = mpsc::channel(32);
let (event_tx, event_rx) = mpsc::channel(1024);

tokio::spawn(runtime::supervisor::run(command_rx, event_tx, deps));

IyonCore { command_tx, event_rx }
```

The client receives only:

```txt
command_tx  # send commands into core
event_rx    # receive events from core
```

The runtime task owns:

```txt
command_rx  # receive commands from clients
event_tx    # emit events to clients
```

## Core Commands

Commands are client-to-core messages.

```rust
pub enum CoreCommand {
    SubmitTurn {
        text: String,
    },

    SubmitSteering {
        text: String,
    },

    SubmitFollowUp {
        text: String,
    },

    CancelActiveTurn,

    ApproveToolCall {
        approval_id: ApprovalId,
    },

    RejectToolCall {
        approval_id: ApprovalId,
        reason: Option<String>,
    },

    SetModel {
        model_id: String,
    },

    RunExtensionCommand {
        command_id: String,
        args: serde_json::Value,
    },

    Shutdown,
}
```

### Steering vs Follow-up

- `SubmitSteering`: inject while the agent is still working, after the current assistant/tool phase reaches a safe boundary.
- `SubmitFollowUp`: run after the agent would otherwise stop.

This mirrors the useful distinction from Pi's agent model.

## Core Events

Events are core-to-client messages.

```rust
pub enum CoreEvent {
    AgentStarted,
    AgentFinished,

    TurnStarted {
        turn_id: TurnId,
    },
    TurnFinished {
        turn_id: TurnId,
    },
    TurnFailed {
        turn_id: TurnId,
        message: String,
    },

    MessageStarted {
        turn_id: TurnId,
        message_id: MessageId,
        role: MessageRole,
    },
    MessageDelta {
        turn_id: TurnId,
        message_id: MessageId,
        delta: MessageDelta,
    },
    MessageFinished {
        turn_id: TurnId,
        message_id: MessageId,
    },

    ToolCallStarted {
        turn_id: TurnId,
        message_id: MessageId,
        tool_call_id: ToolCallId,
        name: String,
        args: serde_json::Value,
    },
    ToolCallUpdated {
        turn_id: TurnId,
        tool_call_id: ToolCallId,
        update: ToolUpdate,
    },
    ToolCallFinished {
        turn_id: TurnId,
        tool_call_id: ToolCallId,
        result: ToolResult,
    },

    ToolApprovalRequested {
        approval_id: ApprovalId,
        turn_id: TurnId,
        tool_call_id: ToolCallId,
        name: String,
        args: serde_json::Value,
    },

    Status {
        message: String,
    },

    RegistryUpdated,
}
```

```rust
pub enum MessageRole {
    User,
    Assistant,
    ToolResult,
    Status,
}
```

```rust
pub enum MessageDelta {
    Text(String),
    Thinking(String),
    ToolCallArguments {
        tool_call_id: ToolCallId,
        delta: String,
    },
}
```

```rust
pub enum ToolUpdate {
    Text(String),
    Progress {
        label: String,
        current: Option<u64>,
        total: Option<u64>,
    },
    Details(serde_json::Value),
}
```

The TUI maps these to presentation state. The core does not emit ratatui lines or UI-specific layout concepts.

---

# Core State Model

## Runtime State

```rust
pub struct RuntimeState {
    pub session: SessionState,
    pub agent: AgentState,
    pub tools: ToolRegistry,
    pub extensions: ExtensionRegistry,
    pub active_turn: Option<ActiveTurn>,
    pub pending_approvals: HashMap<ApprovalId, PendingApproval>,
}
```

`RuntimeState` is owned by the runtime supervisor task. It should not be mutated directly by the TUI.

## Session State

Session state is durable product state.

```rust
pub struct SessionState {
    pub id: SessionId,
    pub cwd: PathBuf,
    pub messages: Vec<AgentMessage>,
    pub model: ModelSelection,
    pub system_prompt: String,
    pub metadata: SessionMetadata,
}
```

```rust
pub struct ModelSelection {
    pub provider: String,
    pub model_id: String,
}
```

## Agent Messages

Agent messages are the core transcript format. They may be richer than provider messages.

```rust
pub enum AgentMessage {
    User {
        id: MessageId,
        content: Vec<ContentBlock>,
        timestamp: SystemTime,
    },

    Assistant {
        id: MessageId,
        content: Vec<ContentBlock>,
        usage: Option<Usage>,
        stop_reason: Option<StopReason>,
        timestamp: SystemTime,
    },

    ToolResult {
        id: MessageId,
        tool_call_id: ToolCallId,
        tool_name: String,
        content: Vec<ContentBlock>,
        details: serde_json::Value,
        is_error: bool,
        timestamp: SystemTime,
    },

    Status {
        id: MessageId,
        text: String,
        timestamp: SystemTime,
    },
}
```

`ContentBlock` may either be re-exported from `iyon-api` or mirrored if core needs richer app-only blocks. The request builder is responsible for lowering `AgentMessage` into `iyon_api::ModelMessage`.

## Agent State

```rust
pub struct AgentState {
    pub is_running: bool,
    pub active_turn_id: Option<TurnId>,
    pub active_message_id: Option<MessageId>,
    pub pending_tool_calls: HashSet<ToolCallId>,
    pub queued_steering: Vec<AgentMessage>,
    pub queued_followups: Vec<AgentMessage>,
    pub error_message: Option<String>,
}
```

## Active Turn

```rust
pub struct ActiveTurn {
    pub id: TurnId,
    pub abort: CancellationToken,
    pub started_at: SystemTime,
}
```

Cancellation is explicit. `CoreCommand::CancelActiveTurn` cancels this token and the agent loop cooperates with it.

---

# Agent Loop

The agent loop is the central orchestration mechanism.

## Responsibilities

- build model requests from a local session snapshot
- consume model stream events
- update partial assistant messages
- assemble streamed tool calls
- execute requested tools
- append assistant/tool-result messages to the local continuation transcript
- continue the model loop after tool results
- return produced messages to the runtime supervisor for durable commit
- handle model/tool errors according to normalized core semantics

## Suggested Modules

```txt
crates/iyon-core/src/agent/
  mod.rs
  loop.rs          # model -> tools -> model loop
  turn.rs          # one model stream response
  state.rs         # AgentState
  transcript.rs    # AgentMessage helpers
  request.rs       # AgentMessage -> iyon_api::ModelRequest
  tool_call.rs     # streamed tool-call assembly
  tool_execution.rs # tool lookup/execution -> AgentMessage::ToolResult
```

## Loop Shape

```txt
run_agent_loop(input):
  clone/use session snapshot from runtime

  loop:
    build ModelRequest from local SessionState + ToolRegistry
    stream one assistant response with run_model_turn
    append final assistant message to local transcript

    match normalized StopReason:
      ToolUse:
        require assembled tool calls
        execute tool calls sequentially
        append tool result messages locally
        continue

      Stop | Length:
        require no assembled tool calls
        emit TurnFinished
        return produced messages

      Error | Aborted:
        fail turn

    if StopReason and tool-call content disagree:
      fail as provider protocol error
```

## Partial Assistant Message Handling

While streaming:

```txt
ModelStreamEvent::Started
  -> create partial assistant AgentMessage
  -> emit MessageStarted

TextDelta
  -> update partial assistant content
  -> emit MessageDelta::Text

ThinkingDelta
  -> update partial assistant content
  -> may emit a semantic thinking delta when the frontend needs it

ToolCallDelta
  -> update ToolCallAssembler
  -> may emit MessageDelta::ToolCall when the frontend needs it

Done/Error
  -> finalize assistant message
  -> emit MessageFinished
```

The finalized assistant message is what stays in `SessionState.messages`.

---

# Tool System

Tools are registered in core. The agent loop only depends on the generic registry/executor interfaces.

## Tool Definition

```rust
pub struct ToolDefinition {
    pub name: String,
    pub label: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub execution_mode: ToolExecutionMode,
    pub approval: ToolApprovalPolicy,
    pub source: ToolSource,
    pub prompt_snippet: Option<String>,
    pub prompt_guidelines: Vec<String>,
}
```

```rust
pub enum ToolExecutionMode {
    Sequential,
    Parallel,
}
```

```rust
pub enum ToolApprovalPolicy {
    NeverAsk,
    AlwaysAsk,
}
```

```rust
pub enum ToolSource {
    Builtin,
    Extension { extension_id: String },
    Sdk,
}
```

`prompt_snippet` and `prompt_guidelines` are definition metadata for later system-prompt construction. They are not sent in `ModelToolSpec` directly. Core intentionally does not store render callbacks on tool definitions; UI rendering remains a frontend concern.

## Tool Executor Trait

```rust
pub type ToolFuture<'a> = Pin<
    Box<dyn Future<Output = anyhow::Result<ToolResult>> + Send + 'a>
>;

pub trait ToolExecutor: Send + Sync {
    fn definition(&self) -> ToolDefinition;

    fn execute<'a>(
        &'a self,
        ctx: ToolContext,
        input: serde_json::Value,
        updates: ToolUpdateSink,
    ) -> ToolFuture<'a>;
}
```

## Tool Context

```rust
pub struct ToolContext {
    pub session_id: SessionId,
    pub turn_id: TurnId,
    pub tool_call_id: ToolCallId,
    pub cwd: PathBuf,
    pub workspace: Workspace,
    pub cancellation: tokio_util::sync::CancellationToken,
}
```

Cancellation is cooperative and flows from runtime to agent loop to model turns and tool execution.

## Tool Result

```rust
pub struct ToolResult {
    pub content: Vec<ContentBlock>,
    pub details: serde_json::Value,
    pub is_error: bool,
    pub terminate: bool,
}
```

`terminate` is a hint that the agent should stop after the current tool batch. If multiple tools ran in a batch, early termination should happen only if every finalized tool result requests termination.

## Tool Registry

```rust
pub struct ToolRegistry {
    tools: BTreeMap<String, Arc<dyn ToolExecutor>>,
    active_tool_names: BTreeSet<String>,
}
```

```rust
impl ToolRegistry {
    pub fn register(&mut self, tool: Arc<dyn ToolExecutor>) -> Result<()>;
    pub fn get(&self, name: &str) -> Option<Arc<dyn ToolExecutor>>;
    pub fn definitions(&self) -> Vec<ToolDefinition>;
    pub fn active_definitions(&self) -> Vec<ToolDefinition>;
    pub fn set_active_tools<I, S>(&mut self, names: I) -> Result<()>;
    pub fn active_tool_names(&self) -> Vec<String>;
    pub fn model_specs(&self) -> Vec<iyon_api::ModelToolSpec>;
}
```

`model_specs()` returns active tools only, in deterministic order. This mirrors the extension-friendly split between all registered tools and the subset currently exposed to the model.

## Tool Execution Flow

```txt
assistant message contains tool calls
  -> find tool in ToolRegistry
  -> prepare/normalize arguments
  -> validate against schema
  -> run before_tool_call extension hooks
  -> check approval policy
  -> maybe emit ToolApprovalRequested and wait
  -> execute tool
  -> emit ToolCallUpdated events from progress callbacks
  -> run after_tool_call extension hooks
  -> emit ToolCallFinished
  -> append ToolResult AgentMessage
```

## Parallel vs Sequential Execution

Default should be parallel, with per-tool override.

Rules:
- If global mode is sequential, run one by one.
- If any tool in the assistant batch requires sequential, run sequentially.
- Otherwise prepare calls sequentially, execute allowed calls concurrently.
- Emit tool result messages back to the model in the assistant's original tool-call order.

This preserves deterministic transcript construction while allowing fast tools to finish independently.

## Builtin Tools

Initial builtin set:

```txt
read
write
edit
bash
grep
find
ls
```

Suggested layout:

```txt
crates/iyon-core/src/tools/
  mod.rs
  registry.rs
  definition.rs
  executor.rs
  approval.rs
  builtin/
    mod.rs
    read.rs
    write.rs
    edit.rs
    bash.rs
    grep.rs
    find.rs
    ls.rs
```

---

# Filesystem Layer

Filesystem safety should be separated from tools.

Tools should not resolve paths manually. They should call the workspace layer.

```txt
crates/iyon-core/src/fs/
  mod.rs
  workspace.rs
  permissions.rs
  read.rs
  write.rs
  search.rs
```

## Workspace

```rust
pub struct Workspace {
    root: PathBuf,
    permissions: FsPermissions,
}
```

```rust
impl Workspace {
    pub fn root(&self) -> &Path;
    pub fn resolve_safe(&self, path: &str) -> Result<PathBuf>;
    pub fn ensure_read_allowed(&self, path: &Path) -> Result<()>;
    pub fn ensure_write_allowed(&self, path: &Path) -> Result<()>;
}
```

## Permissions

```rust
pub struct FsPermissions {
    pub allow_read_outside_root: bool,
    pub allow_write_outside_root: bool,
    pub deny_patterns: Vec<String>,
    pub allow_hidden: bool,
}
```

Default should be conservative:
- no writes outside workspace root
- no path traversal outside root
- explicit approval for destructive operations

---

# Extension System

Extensions should plug into core without changing the agent loop.

Extensions can eventually provide:
- tools
- commands
- model providers
- system prompt fragments
- event hooks
- tool-call hooks
- tool-result hooks
- UI requests through a client capability boundary

The first version should focus on tools and hooks.

## Extension Trait

```rust
pub trait Extension: Send + Sync {
    fn manifest(&self) -> ExtensionManifest;

    fn activate(&self, ctx: ExtensionContext) -> Result<ExtensionRegistration>;
}
```

## Manifest

```rust
pub struct ExtensionManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
}
```

## Registration

```rust
pub struct ExtensionRegistration {
    pub tools: Vec<Arc<dyn ToolExecutor>>,
    pub commands: Vec<ExtensionCommand>,
    pub system_prompt_fragments: Vec<String>,
    pub hooks: Vec<Arc<dyn ExtensionHooks>>,
}
```

## Extension Hooks

```rust
#[async_trait]
pub trait ExtensionHooks: Send + Sync {
    async fn before_model_request(
        &self,
        request: &mut iyon_api::ModelRequest,
    ) -> Result<()> {
        Ok(())
    }

    async fn before_tool_call(
        &self,
        call: &PreparedToolCall,
    ) -> Result<ToolCallHookResult> {
        Ok(ToolCallHookResult::Allow)
    }

    async fn after_tool_call(
        &self,
        call: &PreparedToolCall,
        result: &mut ToolResult,
    ) -> Result<()> {
        Ok(())
    }

    async fn on_core_event(&self, event: &CoreEvent) -> Result<()> {
        Ok(())
    }
}
```

```rust
pub enum ToolCallHookResult {
    Allow,
    Block { reason: String },
}
```

## Extension Context

```rust
pub struct ExtensionContext {
    pub cwd: PathBuf,
    pub session_id: SessionId,
    pub commands: CoreCommandSink,
    pub events: CoreEventSink,
}
```

For external-process plugins later, this boundary can be transported over JSON-RPC or stdio.

Important principle:

```txt
builtin tool and extension tool both become Arc<dyn ToolExecutor>
```

The agent loop should not branch on tool source.

---

# Runtime Supervisor

The runtime supervisor owns the command loop and mutable runtime state.

Suggested layout:

```txt
crates/iyon-core/src/runtime/
  mod.rs
  supervisor.rs      # top-level command loop
  dispatch.rs        # CoreCommand routing
  state.rs           # RuntimeState
  cancellation.rs    # cancellation helpers/tokens
```

## Supervisor Responsibilities

- receive `CoreCommand`
- mutate `RuntimeState`
- spawn/cancel active agent turns
- forward events from agent loop to client event channel
- maintain pending approvals
- handle shutdown

## Dispatch Shape

```txt
CoreCommand::SubmitTurn
  if agent idle:
    start new active turn
  else:
    reject, or queue as follow-up depending policy

CoreCommand::SubmitSteering
  if agent running:
    enqueue steering
  else:
    submit as normal turn or reject depending policy

CoreCommand::CancelActiveTurn
  cancel active token

CoreCommand::ApproveToolCall / RejectToolCall
  resolve pending approval waiter

CoreCommand::Shutdown
  cancel active work and exit supervisor
```

---

# Request Construction

The request builder converts core/session messages into provider-neutral API messages.

```txt
AgentMessage[]
  -> transform context
  -> filter app-only messages
  -> lower to iyon_api::ModelMessage[]
  -> add ModelToolSpec[] from ToolRegistry
  -> apply extension before_model_request hooks
  -> ModelApi::stream(request)
```

Suggested module:

```txt
agent/request.rs
```

Responsibilities:
- deterministic prompt construction
- stable message ordering
- tool spec collection
- context trimming/compaction later
- app-message filtering
- extension request hooks

Prompt caching note:
- keep transcript append-only by default
- keep request construction deterministic
- avoid rewriting old messages unless explicitly compacting or branching

---

# ID Types

Prefer explicit ID newtypes.

```rust
pub struct SessionId(pub u64);
pub struct TurnId(pub u64);
pub struct MessageId(pub u64);
pub struct ToolCallId(pub String);
pub struct ApprovalId(pub u64);
```

Suggested layout:

```txt
crates/iyon-core/src/ids.rs
```

---

# Final Folder Layout

```txt
crates/
  iyon-api/
    src/
      lib.rs
      client.rs
      model.rs
      stream.rs
      tool.rs
      error.rs
      providers/
        mod.rs
        mock.rs
        openai.rs
        anthropic.rs

  iyon-core/
    src/
      lib.rs
      config.rs
      command.rs
      event.rs
      handle.rs
      ids.rs

      runtime/
        mod.rs
        supervisor.rs
        dispatch.rs
        state.rs
        cancellation.rs

      agent/
        mod.rs
        loop.rs
        turn.rs
        state.rs
        transcript.rs
        request.rs
        tool_calls.rs

      tools/
        mod.rs
        registry.rs
        definition.rs
        executor.rs
        approval.rs
        builtin/
          mod.rs
          read.rs
          write.rs
          edit.rs
          bash.rs
          grep.rs
          find.rs
          ls.rs

      fs/
        mod.rs
        workspace.rs
        permissions.rs
        read.rs
        write.rs
        search.rs

      extensions/
        mod.rs
        manifest.rs
        registry.rs
        runner.rs
        hooks.rs

      session/
        mod.rs
        state.rs
        snapshot.rs

  iyon-tui/
    src/
      runtime/
        core_bridge.rs      # optional thin adapter around IyonCore
```

---

# Build Order

## Phase 1: Provider-neutral model stream boundary

- Keep `CoreCommand`/`CoreEvent` and `IyonCore` as the mpsc client/core boundary.
- Use a direct `ModelApi::stream(ModelRequest) -> ModelStreamFuture` API for core/provider communication.
- Make `ModelStream` a boxed async stream of `Result<ModelStreamEvent, ModelError>`.
- Add typed `ModelError`/`ModelErrorKind`.
- Ensure `ModelRequest` includes params and metadata.
- Ensure model-visible tool schemas and tool-call arguments use structured JSON values.
- Add provider-normalized stream events for text, thinking, tool calls, usage, done, and error.
- Keep mock provider streaming through this API without exposing mpsc.
- Keep TUI behavior unchanged.

Acceptance:
- `iyon-core` consumes the mock provider with `StreamExt::next()`
- user submits message in TUI
- core receives command
- mock model streams deltas
- TUI active pane shows assistant streaming
- turn finishes cleanly

## Phase 2: Core IDs, session state, and transcript model

- Add explicit ID newtypes for sessions, turns, messages, tool calls, and approvals.
- Add `SessionState`, `ModelSelection`, and `SessionMetadata`.
- Add the core `AgentMessage` transcript format.
- Add basic `AgentState` placeholders for active turn/message state and future queues.
- Make the runtime supervisor own `SessionState` and ID allocation.
- Append user messages on submit and finalized assistant messages on successful completion.
- Keep request construction temporary and local until Phase 3.

Acceptance:
- core owns a durable transcript independent of provider request shape
- submitted user turns append user transcript entries
- completed mock responses append assistant transcript entries
- existing TUI streaming behavior is unchanged

## Phase 3: Deterministic request builder

- Add `agent/request.rs`.
- Convert `AgentMessage` to `iyon_api::ModelMessage` at the model boundary.
- Build `ModelRequest` from `SessionState`, model params/metadata, and tool specs.
- Preserve transcript ordering deterministically.
- Filter app-only/status messages only in this boundary.
- Runtime builds request snapshots from `SessionState` after appending the submitted user message.
- Turn tasks consume immutable `ModelRequest` snapshots rather than raw user input.

Acceptance:
- second turn includes first turn's transcript in model request
- request construction is deterministic and unit-tested
- status messages are filtered out of provider context
- tool result messages lower to provider-neutral tool result messages

## Phase 4: Agent turn extraction and core-driven message events

- Move model stream consumption from `runtime.rs` into `agent/turn.rs`.
- Keep the runtime supervisor responsible for command dispatch, runtime state, active turn tracking, and transcript commits.
- Keep the agent turn runner responsible for assistant stream handling, assistant accumulation, stream-event mapping, and turn outcome creation.
- Add semantic lifecycle events for agent and message start/finish.
- Core emits user message lifecycle events after committing the user message to `SessionState`.
- TUI presents user messages from core events instead of appending them optimistically on submit.

Acceptance:
- existing mock streaming still works
- runtime no longer owns model stream consumption
- user and assistant presentation are projections of core message events
- no TUI rendering concepts enter core

## Phase 5: Tool registry and filesystem safety foundation

- Add `ToolDefinition`, `ToolExecutor`, `ToolRegistry`, `ToolContext`, `ToolResult`, and `ToolUpdate`.
- Track registered tools and active tool names separately.
- Return model tool specs for active tools in deterministic order.
- Add `Workspace` and `FsPermissions` as the filesystem safety boundary below tools.
- Add one builtin read-only `read` tool using the workspace layer.
- Update request construction to accept a `RequestBuildContext` and advertise active tool specs.
- Runtime owns a tool registry and advertises builtin defaults, but does not execute model-requested tools yet.

Acceptance:
- request builder includes active tool specs from `ToolRegistry`
- builtin `read` is registered and advertised deterministically
- read tool uses `Workspace` safety checks
- no model-requested tool execution happens yet

## Phase 6: Agent loop tool foundation

- Add `agent::tool_call` to assemble streamed model tool calls into ordered tool-call requests.
- Add `agent::tool_execution` to orchestrate lookup/execution and convert tool outcomes into `AgentMessage::ToolResult`.
- Split one model response handling from the full continuation loop:
  - `agent::turn` owns a single provider stream response.
  - `agent::loop` owns model → tools → model continuation.
- Runtime still owns durable `SessionState`; the spawned agent loop mutates a local session snapshot and returns produced messages for runtime commit.
- Runtime seeds the loop with the next message ID and advances the durable counter only after successful commit.
- The agent loop uses normalized `StopReason` as an API contract:
  - `ToolUse` requires tool calls and continues.
  - `Stop`/`Length` require no tool calls and finish.
  - `Error`/`Aborted` fail the turn.
  - stop-reason/content disagreement is a provider protocol error.
- Tool execution errors and missing tools become model-visible tool result messages with `is_error=true`.
- Tool calls execute sequentially for deterministic transcript ordering.

Acceptance:
- model requests a tool with normalized `StopReason::ToolUse`
- core executes builtin `read` through `ToolRegistry`
- assistant tool-call and tool-result messages are appended to the local continuation transcript
- continuation request includes assistant tool call + tool result
- runtime commits loop-produced messages after successful completion

## Phase 7: Approval, cancellation, and tool progress

- Use `tokio_util::sync::CancellationToken` as the cooperative cancellation primitive.
- Runtime owns the active turn cancellation token and forwards per-turn control messages to the agent loop.
- Add approval commands and events for tool calls.
- Enforce `ToolApprovalPolicy::NeverAsk` / `AlwaysAsk` in centralized tool execution.
- Add cancellation to `ModelTurnInput`, `AgentLoopInput`, `ToolContext`, and `ToolUpdateSink`.
- Wire `ToolUpdateSink` to emit semantic `ToolCallUpdated` events.

Acceptance:
- active turn cancellation is token-driven with task abort as fallback
- approval-required tools can pause for approve/reject control messages
- rejected tools produce model-visible error tool results
- tool progress updates flow through core events

## Phase 8: Extension skeleton

- Add `ExtensionRegistry`.
- Support extension-provided tools and before/after tool hooks.

Acceptance:
- builtin and extension tools execute through same path
- hooks can block or modify tool results

## Phase 9: Real providers

- Add one real provider in `iyon-api`.
- Keep provider-specific details out of core.

Acceptance:
- same core/TUI path works with mock and real provider

---

# Architectural Rules

1. TUI never talks to providers directly.
2. TUI never executes tools directly.
3. Providers never know about TUI or filesystem execution.
4. Core owns mpsc setup and backend orchestration.
5. Core events are semantic, not formatted UI rows.
6. `iyon-api` events are model-semantic, not agent/runtime events.
7. Builtin tools and extension tools share one executor interface.
8. Filesystem safety lives below tools in the `fs` layer.
9. Request construction must be deterministic.
10. Cancellation must be explicit and cooperative.

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
    fn stream(&self, request: ModelRequest) -> ModelStream;
}
```

`ModelStream` can be a boxed async stream:

```rust
pub type ModelStream = Pin<
    Box<dyn Stream<Item = Result<ModelStreamEvent, ModelError>> + Send>
>;
```

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

- append user messages
- emit lifecycle events
- build model requests
- consume model stream events
- update partial assistant messages
- assemble streamed tool calls
- execute requested tools
- append tool result messages
- continue the model loop after tool results
- handle cancellation and errors
- drain steering/follow-up queues at safe boundaries

## Suggested Modules

```txt
crates/iyon-core/src/agent/
  mod.rs
  loop.rs          # model -> tools -> model loop
  turn.rs          # one submitted user turn
  state.rs         # AgentState
  transcript.rs    # AgentMessage helpers
  request.rs       # AgentMessage -> iyon_api::ModelRequest
  tool_calls.rs    # streamed tool-call assembly
```

## Loop Shape

```txt
run_turn(input):
  append user message
  emit TurnStarted

  loop:
    drain steering messages, if any
    build ModelRequest
    stream assistant response
    append final assistant message

    if assistant stop reason is error/aborted:
      emit TurnFailed
      return

    if assistant has tool calls:
      execute tool calls
      append tool result messages
      if all tool results requested terminate:
        emit TurnFinished
        return
      continue

    if follow-up queue has messages:
      append follow-up messages
      continue

    emit TurnFinished
    return
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
  -> emit MessageDelta::Thinking

ToolCallDelta
  -> update ToolCallAssembler
  -> emit MessageDelta::ToolCallArguments if useful

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
    Always,
    Never,
    Ask,
}
```

```rust
pub enum ToolSource {
    Builtin,
    Extension { extension_id: String },
}
```

## Tool Executor Trait

```rust
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    fn definition(&self) -> ToolDefinition;

    async fn execute(
        &self,
        ctx: ToolContext,
        input: serde_json::Value,
        updates: ToolUpdateSink,
    ) -> Result<ToolResult>;
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
    pub cancellation: CancellationToken,
}
```

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
    tools: HashMap<String, Arc<dyn ToolExecutor>>,
}
```

```rust
impl ToolRegistry {
    pub fn register(&mut self, tool: Arc<dyn ToolExecutor>) -> Result<()>;
    pub fn get(&self, name: &str) -> Option<Arc<dyn ToolExecutor>>;
    pub fn model_specs(&self) -> Vec<iyon_api::ModelToolSpec>;
}
```

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

## Phase 1: Core channel boundary and mock model

- Define `CoreCommand`, `CoreEvent`, `IyonCore`.
- Make `iyon_core::spawn()` own mpsc setup.
- Add `iyon-api` mock provider that streams text deltas.
- Wire TUI to `IyonCore` instead of local placeholder backend events.

Acceptance:
- user submits message in TUI
- core receives command
- mock model streams deltas
- TUI active pane shows assistant streaming
- turn finishes cleanly

## Phase 2: Agent transcript and request builder

- Add `SessionState` and `AgentMessage`.
- Convert `AgentMessage` to `ModelRequest`.
- Preserve assistant final messages in session transcript.

Acceptance:
- second turn includes first turn's transcript in model request
- request construction is deterministic

## Phase 3: Tool call assembly and registry

- Add `ToolRegistry` and `ToolExecutor`.
- Add streamed tool-call assembler.
- Add one simple builtin read-only tool.
- Add model loop continuation after tool result.

Acceptance:
- mock model requests a tool
- core executes tool
- tool result is appended
- model loop continues

## Phase 4: Approval and cancellation

- Add approval policy.
- Add pending approval state.
- Add cancellation token support.

Acceptance:
- dangerous tool can request approval
- TUI can approve/reject
- cancel stops active turn deterministically

## Phase 5: Extension skeleton

- Add `ExtensionRegistry`.
- Support extension-provided tools and before/after tool hooks.

Acceptance:
- builtin and extension tools execute through same path
- hooks can block or modify tool results

## Phase 6: Real providers

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

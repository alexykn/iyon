# Backend Architecture (High Level)

This document defines the backend runtime shape for `iyon` and how it connects to the frontend loop.

## Target Pipeline

`provider-api (real impl, async)` -> `generic api (async abstraction)` -> `backend state machine (regular async + mpsc)` -> `frontend event queue` -> `frontend formatter` -> `renderer + scrollback commit`

## Layer Responsibilities

## 1) Provider API (real implementation, async)

Examples: OpenAI, Anthropic, local model server.

Responsibilities:
- Perform provider-specific HTTP/SSE/WebSocket calls.
- Handle provider auth headers, endpoint details, request/response schema.
- Apply provider-local retry/backoff for transient transport/status failures.
- Stream provider chunks asynchronously.

Rules:
- No UI knowledge.
- No ratatui knowledge.
- No app-level state mutation.

## 2) Generic API (async abstraction)

Purpose:
- Hide provider-specific differences behind one internal interface.
- Normalize provider-native stream shapes into a unified semantic stream.

Responsibilities:
- Convert unified request model to provider request.
- Normalize provider stream output into unified stream events.
- Normalize provider failures into `ModelErrorKind` (`Authentication`, `RateLimited`, `InvalidRequest`, etc.).

Important:
- This layer is an adapter/normalizer, not a UI formatter.
- It should not produce ratatui-specific output or width/theme-dependent formatting.

Implemented trait shape:

```rust
use std::{future::Future, pin::Pin};

use futures_core::Stream;

pub type ModelStream = Pin<
    Box<dyn Stream<Item = Result<ModelStreamEvent, ModelError>> + Send>
>;

pub type ModelStreamFuture<'a> = Pin<
    Box<dyn Future<Output = Result<ModelStream, ModelError>> + Send + 'a>
>;

pub trait ModelApi: Send + Sync {
    fn stream(&self, request: ModelRequest) -> ModelStreamFuture<'_>;
}
```

`ModelStreamEvent` should be provider-agnostic and model-semantic, not UI formatted.

`Done { stop_reason }` is part of the generic API contract. Provider adapters must normalize native finish reasons into `StopReason` and keep the final reason consistent with final assistant content. If the response contains tool calls, adapters should report `StopReason::ToolUse`, following the same normalizing pattern used by Pi's provider layer.

`ModelStreamEvent` is provider-semantic and includes:
- `Started`
- `TextStart/TextDelta/TextEnd`
- `ThinkingStart/ThinkingDelta/ThinkingEnd`
- `ToolCallStart/ToolCallDelta/ToolCallEnd`
- `Usage`
- `Done`
- `Error { message }`

Why `Stream` as default:
- Avoid hidden `tokio::spawn` bridging tasks just to return channel receivers.
- Better composability (`map`, `filter`, `timeout`, `select`, etc.).
- Clearer producer/consumer lifecycle and backpressure behavior.

Fallback:
- Returning `tokio::sync::mpsc::Receiver<_>` is allowed when the provider integration is naturally channel/callback-driven or needs explicit fan-in.

## 3) Backend State Machine (regular async + channels)

This is orchestration logic, not provider transport.

Responsibilities:
- Receive frontend commands through `mpsc`.
- Own durable `SessionState` and runtime turn lifecycle.
- Build model requests from conversation/session state.
- Spawn an agent loop for each submitted turn.
- Commit completed agent-loop messages back into durable session state.
- Emit frontend-facing/core events through `mpsc`.

The agent loop owns transient per-turn continuation state:
- clone the session snapshot at turn start
- call the generic model API
- execute tools through `ToolRegistry`
- run before/after tool hooks from a per-turn immutable hook snapshot
- append assistant/tool-result messages to the local snapshot
- continue model calls until normalized `StopReason` indicates completion
- return produced messages to the runtime supervisor for durable commit

Important:
- This can be implemented as normal async control flow (no giant manual enum required).
- Use `tokio::select!` for cancellation and concurrent command handling.

Command channel (frontend -> backend):

```rust
pub enum BackendCommand {
    SubmitTurn { input: String },
    CancelActiveTurn,
    // later: RetryTurn, ApproveToolCall, etc.
}
```

Event channel (backend -> frontend):

```rust
pub enum FrontendEvent {
    TurnStarted { turn_id: u64 },
    Delta { turn_id: u64, chunk: StreamChunk },
    TurnFinished { turn_id: u64 },
    TurnFailed { turn_id: u64, message: String },
}
```

The current frontend adapter shape is intentionally smaller than the core event model while backend metadata is still fluid:

```rust
pub(crate) enum BackendCommand {
    SubmitTurn { text: String },
    CancelActiveTurn,
}

pub(crate) enum FrontendEvent {
    TurnStarted,
    UserMessage { text: String },
    AssistantDelta { text: String },
    TurnFinished,
    TurnFailed { message: String },
}
```

The placeholder should evolve toward the metadata-rich shape above once the backend turn model is implemented.

Frontend composition should interpret these events with explicit presentation transitions:
- `UserMessage` -> append a user presentation item projected from core state.
- `TurnStarted` -> show ephemeral `WorkingSpinner` pane above input.
- First assistant delta for the turn -> replace spinner with `StreamingAssistant` pane.
- Subsequent assistant deltas -> update the same pane in place.
- Tool call/result events -> append/update canonical tool presentation items in `TranscriptState`; active pane may show transient status/approval only.
- `TurnFinished`/`TurnFailed` -> finalize pane into a normal transcript item (or error item).

This keeps backend transport semantic and UI-agnostic while still giving the frontend a deterministic state machine.

Model-visible tool output and TUI presentation are separate policies. Core/tool code may bound generated outputs before appending them to `SessionState`, with explicit metadata. TUI formatting may apply a different display-only truncation/collapse policy without changing the semantic transcript. Core tool events include tool identity plus structured call arguments/result details so the TUI can render builtin-specific presentations without core sending styles. Read-only default builtins currently include `read`, `ls`, `find`, and `grep`; search tools prefer Pi-like external tools (`fd`, `rg`) and fall back to system tools (`find`, `grep`). Mutation/process builtins `write`, `edit`, and `bash` run yolo by default; `write`/`edit` share a per-path mutation queue, while `bash` streams progress and model-truncates final output from the tail. `bash` commands containing a standalone `sudo` token still require approval.

## 4) Formatter Stage

Purpose:
- Convert backend/domain events into display-ready output for UI + scrollback parity.

Responsibilities:
- Map semantic events (`AssistantDelta`, `ToolStarted`, `DiffChunk`) to styled lines.
- Produce shared formatted output (recommended: `Vec<Line<'static>>`).

Where it runs:
- In frontend composition/event handling after receiving `FrontendEvent`.
- It is not a backend responsibility.

Constraint:
- Keep one canonical formatting path per item type.
- Do not duplicate formatting logic across live renderer and scrollback path.
- Keep formatting width-agnostic (style/content only).
- Keep width/wrapping/viewport/line-height logic in render/commit stages.

Clarification:
- Provider -> generic API uses a normalizer/adapter.
- Frontend formatter maps semantic events to styled lines; renderer applies geometry rules.

## 5) Frontend (sync ratatui loop + event queue)

Responsibilities:
- Poll input events and convert them to frontend actions.
- Send `BackendCommand` through `BackendEventHandler` (non-blocking/fire-and-forget).
- Drain backend event queue through `BackendEventHandler::drain_actions()`.
- Apply frontend actions through `AppController`.
- Update app state and render current state synchronously only when dirty.

Current frontend modules:
- `InputEventHandler`: terminal input -> `FrontendAction`.
- `BackendEventHandler`: `mpsc` command/event bridge -> `FrontendAction`.
- `AppController`: `FrontendAction` -> `AppState` mutation and backend command sends.
- `App`: orchestration of wait/poll/drain/tick/commit/draw.

Pattern:
- Frontend never blocks on network.
- Backend never calls UI directly.
- Communication happens only through typed messages.
- UI-specific transitions stay in frontend composition (`AppController`/`AppState`), not backend transport.

## Data Flow (One Turn)

1. User submits input.
2. `InputEventHandler` emits `FrontendAction::SubmitTurn { text }`.
3. `AppController` sends `BackendCommand::SubmitTurn` through `BackendEventHandler`; it does not commit a conversation transcript item optimistically.
4. Backend appends the submitted user message to `SessionState` and emits user message lifecycle events.
5. Frontend projects those core events into a user presentation item.
6. Backend clones a session snapshot, tool registry snapshot, workspace, and next message ID into the spawned agent loop.
7. Agent loop builds immutable `ModelRequest` snapshots from its local `SessionState` via the deterministic request builder.
8. Agent loop calls `ModelApi::stream(req).await` for one model response.
9. Agent loop emits assistant message lifecycle/delta events while assembling final assistant content.
10. If `StopReason::ToolUse` is returned with tool calls, agent loop executes tools through `ToolRegistry`, appends tool results locally, and builds a continuation request.
11. If `StopReason::Stop` or `StopReason::Length` is returned without tool calls, agent loop completes.
12. Runtime commits produced assistant/tool-result messages to durable `SessionState` and advances the durable message ID counter.
13. Frontend drains backend events, maps them to `FrontendEvent`s, updates active pane/transcript state, and runs formatting as needed.
14. Frontend renders when dirty.
15. On completion/failure, backend emits terminal event (`TurnFinished`/`TurnFailed`), and frontend finalizes/fails the active pane.

## State Ownership

Frontend owns:
- UI state (focus, scroll, cursor position, pane geometry).
- Active pane, markdown formatting, scrollback, and render cache.
- A presentation transcript projected from core events, not the authoritative conversation transcript.

Backend owns:
- Session/turn state for API interaction.
- `SessionState` and the provider-agnostic `AgentMessage` transcript.
- In-flight request lifecycle and cancellation.
- Provider-agnostic transcript for request construction and prompt caching strategy.

## Prompt Caching Note

To maximize prefix reuse:
- Keep transcript history append-only by default.
- If user edits past content, fork or truncate from that point explicitly.
- Keep request construction deterministic so equal prefix stays byte-stable.

## Concurrency Model

- Frontend: sync event/render loop.
- Backend: async orchestration task(s).
- Provider/generic API: async stream pipeline.
- Channels: bounded `tokio::sync::mpsc` for backpressure.

Suggested starting bounds:
- `frontend -> backend` command queue: small (`8-32`).
- `backend -> frontend` event queue: moderate (`256-2048`) depending on delta volume.

## Failure/Cancellation

- All network/provider errors must map to typed backend/frontend events.
- Cancellation is explicit command (`CancelActiveTurn`) and uses `tokio_util::sync::CancellationToken` through runtime, agent loop, model turn, and tool execution.
- Runtime signals cooperative cancellation and may abort the task as a fallback while provider/tool cancellation support matures.
- Dropping stream/request resources should terminate active generation deterministically.

## Minimal Rust Skeleton

```rust
use futures::StreamExt;

// startup wiring
let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel::<BackendCommand>(32);
let (evt_tx, evt_rx) = tokio::sync::mpsc::channel::<FrontendEvent>(1024);

tokio::spawn(run_backend(cmd_rx, evt_tx, model_api));

// frontend loop (sync):
// - InputEventHandler emits FrontendAction values
// - AppController sends cmd_tx on SubmitTurn
// - BackendEventHandler drains evt_rx with try_recv and emits FrontendAction values
// - AppController updates AppState
// - App draws only when dirty

// backend stream consumption sketch:
// let mut s = model_api.stream(req).await?;
// while let Some(item) = s.next().await {
//     match item {
//         Ok(ev) => { /* map -> FrontendEvent and send on evt_tx */ }
//         Err(e) => { /* emit TurnFailed */ }
//     }
// }
```

## Extension Path

1. Add tool-call approval flow (`BackendCommand::ApproveToolCall`).
2. Add structured usage/cost tracking events.
3. Add persistence snapshots for session replay.
4. Add inline scrollback commit manager (advanced mode).
5. Add provider failover/retry policy in generic API layer.

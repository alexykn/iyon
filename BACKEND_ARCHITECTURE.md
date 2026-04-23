# Backend Architecture (High Level)

This document defines the backend runtime shape for `iyon` and how it connects to the frontend loop.

## Target Pipeline

`provider-api (real impl, async)` -> `generic api (async abstraction)` -> `backend state machine (regular async + mpsc)` -> `frontend event queue` -> `frontend formatter` -> `renderer + scrollback commit`

## Layer Responsibilities

## 1) Provider API (real implementation, async)

Examples: OpenAI, Anthropic, local model server.

Responsibilities:
- Perform provider-specific HTTP/SSE/WebSocket calls.
- Handle provider auth, endpoint details, request/response schema.
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
- Normalize errors.

Important:
- This layer is an adapter/normalizer, not a UI formatter.
- It should not produce ratatui-specific output or width/theme-dependent formatting.

Example trait shape:

```rust
use futures::stream::BoxStream;

pub trait ModelApi {
    async fn stream(
        &self,
        req: ModelRequest,
    ) -> anyhow::Result<BoxStream<'static, anyhow::Result<ModelStreamEvent>>>;
}
```

`ModelStreamEvent` should be provider-agnostic (for example `Token`, `ToolCallDelta`, `Done`, `Usage`, `Error`).

Example `ModelStreamEvent` intent:
- `TextDelta { text }`
- `ToolCallDelta { id, name, args_delta }`
- `Usage { input_tokens, output_tokens }`
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
- Build model request from conversation/session state.
- Call generic API asynchronously.
- Consume stream events and update backend session state.
- Emit frontend-facing events through `mpsc`.

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

Current frontend placeholder shape is intentionally smaller while backend metadata is still fluid:

```rust
pub(crate) enum BackendCommand {
    SubmitTurn { text: String },
    CancelActiveTurn,
}

pub(crate) enum FrontendEvent {
    TurnStarted,
    AssistantDelta { text: String },
    TurnFinished,
    TurnFailed { message: String },
}
```

The placeholder should evolve toward the metadata-rich shape above once the backend turn model is implemented.

Frontend composition should interpret these events with explicit transient-pane transitions:
- `TurnStarted` -> show ephemeral `WorkingSpinner` pane above input.
- First `Delta` for the turn -> replace spinner with `StreamingAssistant` pane.
- Subsequent `Delta` -> update the same pane in place.
- `TurnFinished`/`TurnFailed` -> finalize pane into a normal transcript item (or error item).

This keeps backend transport semantic and UI-agnostic while still giving the frontend a deterministic state machine.

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
3. `AppController` commits the user message to transcript, starts `WorkingSpinner`, and sends `BackendCommand::SubmitTurn` through `BackendEventHandler`.
4. Backend builds `ModelRequest` from session state.
5. Backend calls `ModelApi::stream(req).await`.
6. Backend receives stream events and emits frontend events (`AssistantDelta`/future `Delta` updates).
7. Frontend drains backend events, maps them to `FrontendAction`s, updates active pane/transcript state, and runs formatting as needed.
8. Frontend renders when dirty.
9. On completion/failure, backend emits terminal event (`TurnFinished`/`TurnFailed`), and frontend finalizes/fails the active pane.

## State Ownership

Frontend owns:
- UI state (focus, scroll, cursor position, pane geometry).
- Render cache / formatted view model for display.

Backend owns:
- Session/turn state for API interaction.
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
- Cancellation should be explicit command (`CancelActiveTurn`) and handled via `tokio::select!`.
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

# Render Architecture

This document is the source of truth for the current `iyon-tui` render architecture.
It reflects what is implemented today in code, plus clearly marked near-term evolution points.

---

## Current Architecture (Implemented)

## 1) High-level pipeline

Runtime flow today:

1. `App` orchestrates a dirty-driven sync loop.
2. `InputEventHandler` converts terminal input into `FrontendAction`s, mutating only local input-buffer state for editing/navigation.
3. `BackendEventHandler` drains backend `mpsc` events into `FrontendAction`s.
4. `AppController` applies actions to `AppState` and sends backend commands.
5. Transcript canonical items are formatted to `Vec<Line<'static>>` through `TuiFormatter`.
6. `TranscriptState` caches rendered rows by width.
7. Layout is computed (including active pane reservation and commit-capacity policy).
8. Overflow rows are committed to terminal scrollback.
9. Live frame is rendered via typed views + `Renderer`.

Key property:
- Live rendering and scrollback insertion both consume the same formatted row type (`Line<'static>`), preserving parity.

---

## 2) App loop and shutdown model

`App::run()` is a step/state-machine loop, not a single loop with ad-hoc teardown.

```rust
ExitState::Running
-> Requested
-> FinalizeActive
-> FlushHistory
-> FinalFrame
-> Done
```

### Running phase

- Draws an initial frame, then redraws only when dirty.
- Polls terminal input with a timeout derived from active animation/backend state.
- Converts input and backend messages into `FrontendAction`s.
- Applies actions through `AppController`.
- Ticks active animations only when due.
- Ensures transcript render cache for current width before drawing.
- Commits transcript overflow rows according to commit capacity.
- Draws running UI only when input/backend/tick/resize changed state.

### Shutdown phases

- `FinalizeActive`: resolve active pane behavior (`SpillToTranscript` or `OccludeOnly`).
- `FlushHistory`: commit remaining uncommitted transcript rows in chunks.
- `FinalFrame`: draw exit anchor, insert goodbye rows, set cursor position, then `Done`.

This keeps shutdown deterministic and avoids dropping transcript rows.

---

## 3) State model

## `AppState`

- `input: InputBuffer`
- `transcript: TranscriptState`
- `active: Option<ActivePaneState>`
- `info: InfoState`
- `input_content_width: u16`
- `scroll: u16`
- `exit_state: ExitState`

## `TranscriptState`

- `canonical_items: Vec<TimelineItem>`
- `rendered_rows_cache: Option<TranscriptRenderCache>` keyed by width
- `commit_boundary: TranscriptCommitBoundary` monotonic logical/span/byte boundary into formatted transcript rows
- `formatter: TuiFormatter`

Important behavior:
- Width change => cache recompute from canonical items and the monotonic commit boundary.
- New item invalidates the rendered-row cache.
- `uncommitted_rows()` is the source for both chat display and scrollback overflow commit.

## Active pane state

Active pane state lives in `core::active`:

- `ActivePaneKind`: `WorkingSpinner | AgentStreaming | SlashMenu | FilePicker`
- `ActiveBehavior`: `SpillToTranscript | OccludeOnly`
- `ActiveStreamState { full_text, frozen_until }` for spill-capable panes
- tiny internal spinner frame state for `WorkingSpinner`

Current behavior:
- On submit, the TUI sends `CoreCommand::SubmitTurn`; the user message is appended to presentation transcript only when projected back from core message events.
- `TurnStarted` creates a `WorkingSpinner` active pane.
- `WorkingSpinner` renders as a three-row pane without borders: blank/text/blank.
- `AgentStreaming` can render `active_tail()` once backend deltas arrive.
- Active -> transcript spill is still a near-term step; finalization appends only `full_text[frozen_until..]` to avoid duplication.

## Event/control modules

- `InputEventHandler`: terminal input -> `FrontendAction`s.
- `BackendEventHandler`: backend `mpsc` events -> `FrontendAction`s and nonblocking backend command sends.
- `AppController`: `FrontendAction` -> `AppState` mutation plus backend command emission.
- `App`: wait/poll/tick/commit/draw orchestration only.

---

## 4) View + renderer boundaries

Typed view contracts:

- `SpacerView`
- `InputView { text, cursor_bytes, scroll_rows, wrapped_ranges }`
- `ChatView { lines }`
- `ActiveView { active }`
- `InfoView { status }`

Core trait:

```rust
pub(crate) trait Renderable<V: View> {
    fn render(&self, view: &V, frame: &mut Frame, area: Rect);
    fn desired_height(&self, _: &V, area: Rect, input: &str) -> u16 { ... }
}
```

### Notable implementation details

- `InputView` uses precomputed wrapped ranges (`WrapCache`) and explicit `scroll_rows`.
- Cursor placement is computed from wrapped lines (`cursor_xy`) and clamped to area bounds.
- `ChatView` paints directly into the frame buffer and only draws visible tail rows.
- `ActiveView` renders the current active pane. The working spinner uses internal frames and no pane borders.
- `Renderer::draw_running()` composes spacer/chat/active/input/info in one place.

---

## 5) Wrapping + input performance model

`WrapCache` caches wrapped ranges by `(text_revision, width)`.

- Input edits increment `text_revision`.
- If neither revision nor width changed, wrapped ranges are reused.
- Input navigation (including visual up/down) reuses cached wrapped ranges.

This removes repeated re-wrap work on every draw.

---

## 6) Layout and capacity policy

Layout stack (top to bottom):

1. spacer
2. chat
3. active
4. input
5. info

Layout is computed in multiple passes:

- pass 1: base constraints
- pass 2: refine with renderer-derived `input_height`
- final: refine with renderer-derived `chat_height`

Computed layout exposes two capacities:

- `visual_chat_capacity_rows`: rows currently visible in chat area
- `commit_chat_capacity_rows`: rows considered by overflow commit logic

Policy:

- `SpillToTranscript` => commit capacity follows visual chat capacity
- `OccludeOnly` => commit capacity ignores active-pane occlusion

This prevents transient occluding panes (menus/pickers) from causing irreversible scrollback commits.

---

## 7) Scrollback + terminal boundary

`InlineTerminal` is the terminal boundary wrapper:

- `draw(...)`
- `insert_history_rows(...)`
- `last_viewport_area()`
- `invalidate_next_draw()`

`ScrollbackCommitter::commit_rows(...)` performs row insertion and returns diagnostics.

Commit behavior in app:

- Running phase commits transcript overflow rows.
- Flush phase commits remaining rows in bounded chunks.
- Mismatch/truncation is treated as error (not silently swallowed).

---

## 8) Misalignments fixed from older render doc

The architecture is now more mature than the earlier version. Main corrected points:

1. **Exit is a state machine**, not a post-loop cleanup block.
2. **Transcript has canonical + cached + committed boundaries**, not just ad-hoc line vectors.
3. **Layout uses split capacities** (`visual` vs `commit`) tied to active behavior policy.
4. **Input rendering depends on wrap cache + scroll rows** (not only raw text + cursor index).
5. **Chat rendering uses direct buffer painting of visible rows**, not generic paragraph clone/render.
6. **Terminal boundary is explicit** (`InlineTerminal`) with redraw invalidation after history insertion.
7. **Scrollback commit is row-primitive with diagnostics**, chunked for safety.

---

## 9) Event loop for streaming/spinners (implemented foundation)

The running loop is now dirty-driven rather than an unconditional redraw loop.

Sources:

- Input events (keys, paste, resize) through `InputEventHandler`.
- Backend events through `BackendEventHandler::drain_actions()`.
- Periodic active ticks through `ActiveTicker`.

Behavior:

- Initial frame draws immediately.
- Without active animation or backend in-flight state, the input poll timeout is long and idle redraws stop.
- While `WorkingSpinner` is active, the loop wakes at the spinner tick rate and redraws only when the spinner advances.
- While a backend turn is in flight, the loop also wakes at `BACKEND_POLL_INTERVAL` to drain `mpsc` events, but redraws only if events were received/applied.
- Backend event ingestion is structurally wired through `BackendEventHandler`, backed by `iyon-core` and the mock `iyon-api` provider.

In crossterm-style sync code this remains:

- `event::poll(timeout)` + `event::read()` for terminal input.
- nonblocking drain of a channel for stream deltas.
- tick active animations only when due.

---

## 10) Next concrete steps

1. Preserve `BackendEventHandler` as the thin adapter from semantic `CoreEvent` values into TUI-local `FrontendEvent` values.
2. Add richer backend payload metadata (`turn_id`, message ids, chunk shape, error shape) without leaking UI concepts into backend code.
3. Implement active streaming spill (`full_text` + monotonic `frozen_until`) into transcript.
4. Add markdown/live formatting for streaming assistant content while preserving the shared canonical formatting path.
5. Add occluding pane constructors/rendering/disengage only when menu/file-picker UX is actually implemented.

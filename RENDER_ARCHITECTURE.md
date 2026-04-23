# Render Architecture

This document is the source of truth for the current `iyon-tui` render architecture.
It reflects what is implemented today in code, plus clearly marked near-term evolution points.

---

## Current Architecture (Implemented)

## 1) High-level pipeline

Runtime flow today:

1. Input/event handling mutates `AppState`.
2. Transcript canonical items are formatted to `Vec<Line<'static>>` through `TuiFormatter`.
3. `TranscriptState` caches rendered rows by width.
4. Layout is computed (including active pane reservation and commit-capacity policy).
5. Overflow rows are committed to terminal scrollback.
6. Live frame is rendered via typed views + `Renderer`.

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

- Ensures transcript render cache for current width.
- Commits transcript overflow rows according to commit capacity.
- Draws running UI.
- Processes input events.

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
- `committed_rows: usize` monotonic boundary into cached rows
- `formatter: TuiFormatter`

Important behavior:
- Width change => full cache recompute from canonical items.
- New item + existing cache => incremental append of formatted rows.
- `uncommitted_rows()` is the source for both chat display and scrollback overflow commit.

## Active pane state

- `ActivePaneKind`: `WorkingSpinner | AgentStreaming | SlashMenu | FilePicker`
- `ActiveBehavior`: `SpillToTranscript | OccludeOnly`
- `spill_text: String`

Note: active-pane rendering/streaming is only partially wired today (layout/state exist; rendering is currently placeholder in `active_area`).

---

## 4) View + renderer boundaries

Typed view contracts:

- `SpacerView`
- `InputView { text, cursor_bytes, scroll_rows, wrapped_ranges }`
- `ChatView { lines }`
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

## 9) Event-loop evolution for streaming/spinners (recommended)

Current input path uses blocking input reads, which is fine for purely input-driven redraws.
For streaming assistant responses and spinner animation, use a hybrid loop.

You generally want a **hybrid loop**:

- Input events (keys, paste, resize)
- Backend events (token delta, tool updates)
- Periodic ticks (spinner/frame animation, cursor blink, etc.)

### Common fix

Replace “block forever on input” with a select-style wait:

- Wait up to e.g. 16–50ms
- If input arrives: handle it
- If backend token arrives: append/update active pane and redraw
- If timeout expires: tick spinner and redraw only if dirty

In crossterm-style sync code, this is usually:

- `event::poll(timeout)` + `event::read()` for terminal input
- nonblocking drain of a channel for stream deltas
- tick on timeout

In async code, it’s `tokio::select!` over input, stream channel, and `interval.tick()`.

---

## 10) Next concrete steps

1. Wire backend event ingestion into `Running` phase.
2. Render real active panes in `active_area` (spinner/stream/menu).
3. Implement active streaming spill (`full_text` + monotonic frozen boundary) into transcript.
4. Add dirty flags to avoid unnecessary redraw/parse on ticks.
5. Keep canonical formatting path shared across live and commit sinks.

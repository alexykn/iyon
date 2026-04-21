# Active Pane + Streaming Markdown Implementation Plan

This is the implementation plan for adding a transient active pane with seamless streaming into transcript + native scrollback.

It captures the decisions we made so far and turns them into a build checklist.

---

## 0) Goals

- Keep input pinned at bottom.
- Show user message immediately in transcript when submitted.
- Show an active pane right below that message (`Working...` first, then streaming assistant).
- Stream long assistant output smoothly from active pane into transcript and then into native scrollback.
- Render markdown while streaming (not only when finalized).
- Support other active-pane UX (slash menus, `@` pickers, selectors) that can be large and temporarily push chat out of view **without** committing those hidden rows to native scrollback.
- Support menu disengage: after repeated upward scroll intent (for example `3` steps), close the transient menu pane and pull transcript back down.
- Keep one canonical formatting pipeline (`Vec<Line<Span>>` conceptually) for active/chat/scrollback parity.

---

## 1) Non-negotiable invariants

1. **User messages are never in active pane.**
   - On submit, user message is committed directly to transcript.

2. **Active pane is transient and bottom-anchored.**
   - It lives between chat and input.
   - Default height is `3` when active.
   - Height is `0` when inactive.

3. **Canonical text is source of truth.**
   - Do not use width-wrapped lines as canonical state.
   - Wrapped lines are render artifacts.

4. **Monotonic boundaries only move forward.**
   - Active -> transcript boundary (`frozen_until`) never decreases.
   - Transcript -> scrollback commit boundaries never decrease.

5. **Markdown parse runs only on new content.**
   - Not on spinner ticks.

6. **Active pane behavior is explicit per kind.**
   - `AgentStreaming`-style panes may spill into transcript.
   - Menu/selector panes are occluding UI only (no spill to transcript).

7. **Occluding pane visibility must not force scrollback commits.**
   - If chat is pushed out only because a temporary menu is open, do not commit those rows to native scrollback.

---

## 2) Layout model

Use your spacer approach (top-push layout), no custom bottom-up solver required.

Stack:

1. `Spacer (Min(0))`
2. `Chat transcript`
3. `Active pane`
4. `Input`
5. `Info/status`

Behavior:
- When active is `None`: active area height = `0`.
- When active exists: active area height = default `3` (can later clamp-grow).
- As active/input grow, chat area shrinks first (because top spacer is flexible and chat is above active/input).

---

## 3) State model (conceptual)

## App-level
- `transcript` (committed canonical messages)
- `active: Option<ActivePaneState>`
- `pending_scrollback` (formatted committed lines waiting for `insert_before`)
- existing input/editor state

## Active pane
- `kind`: `WorkingSpinner | AgentStreaming | SlashMenu | FilePicker | ...`
- `behavior policy` per kind:
  - `SpillToTranscript` (streaming assistant flow)
  - `OccludeOnly` (menus/selectors; never spill)
- streaming payload (used only by spilling kinds):
  - `full_text`: canonical assistant text received so far
  - `frozen_until`: byte index in `full_text`
    - prefix `[0..frozen_until)` already promoted to transcript
    - suffix `[frozen_until..]` remains active tail
- menu disengage state (used only by occluding kinds):
  - `scroll_up_disengage_count`
  - `close_threshold` (e.g. `3`)
- optional render cache by width

## Transcript
- canonical items (role + text + metadata)
- optional per-item render cache by width

Important: transcript stores canonical content, not permanently wrapped rows.

---

## 4) Turn lifecycle

## 4.1 User submits

1. Validate input (ignore empty if desired).
2. Push `UserMessage` into transcript immediately.
3. Clear input.
4. Create active pane:
   - `kind = WorkingSpinner`
   - `height = 3`
   - `full_text = ""`
   - `frozen_until = 0`

## 4.2 First assistant delta

1. Switch active kind to `AgentStreaming`.
2. Append chunk to `full_text`.
3. Reparse markdown from current `full_text`.
4. Recompute wrapped visual lines for current width.
5. Spill stale top visual lines into transcript by advancing `frozen_until`.
6. Render active tail in active pane.

## 4.3 Subsequent deltas

Repeat same streaming step as above.

## 4.4 Finish / abort / error

1. Finalize any remaining active tail into transcript.
2. If aborted/error, append status marker/message to transcript item.
3. Clear active pane (`None`, height 0).
4. Normal transcript overflow rules continue to scrollback.

---

## 5) Streaming spill algorithm (core)

Given current active `full_text`, active pane height clamp `H`, width `W`:

1. Wrap/render current active content at `W` to visual ranges/lines.
2. If visual line count `<= H`: nothing spills.
3. If visual line count `> H`:
   - `cutoff_line = total_lines - H`
   - `new_frozen = byte_start_of_visual_line(cutoff_line)`
4. If `new_frozen > frozen_until`:
   - promote canonical slice `full_text[frozen_until..new_frozen]` to transcript assistant message
   - set `frozen_until = new_frozen`

Why this works:
- You spill only stale top visual content.
- You don’t mutate canonical text.
- You avoid chopping in middle of a wrapped visual line.

Note: `new_frozen` does **not** need to be on actual `\n` bytes. Soft wraps are valid visual boundaries.

---

## 6) Markdown while streaming

Chosen strategy:

- Parse markdown from the full currently received assistant text whenever new chunk arrives.
- Rebuild styled lines/spans from that parse.
- Use same formatter output model for active + transcript + scrollback commit path.

This matches what mature CLIs do in practice:
- reparsing on updates
- caching by `(text, width)`
- no parser work on animation ticks

Tick behavior:
- Spinner animation only.
- No markdown parse, no formatter rebuild unless width changed.

---

## 7) Render/dirty policy

Use two independent dirty concepts:

1. `content_dirty`
   - set on new streaming chunk
   - do markdown parse + styled line rebuild

2. `visual_dirty`
   - set on width resize, scroll move, spinner tick
   - do wrapping/viewport recompute and repaint
   - no markdown parse unless content changed

This keeps streaming responsive and avoids parse work during spinner frames.

---

## 8) Active pane kinds

Start with one slot and enum-like modes, but split them into two behavior classes.

## 8.1 Spill-capable kinds (transcript-producing)
- `WorkingSpinner`
- `AgentStreaming`
- later maybe tool-progress panes that should become transcript history

Rules:
- can participate in active->transcript spill behavior
- can indirectly drive transcript overflow commit (normal history behavior)

## 8.2 Occluding kinds (UI-only)
- `/` command menus
- `@` file pickers
- selectors, quick-open UI, temporary overlays rendered in active slot

Rules:
- never spill into transcript
- never directly commit anything to native scrollback
- may reduce visual chat viewport temporarily
- when closed, transcript reflows back down from canonical state

Shared rules for both classes:
- same active slot in layout
- same default active height contract (with optional growth/clamp per kind)
- smooth transition (no structural layout changes beyond content)

No sticky footer yet.

---

## 9) Transcript and scrollback correctness

Data flow is one-way only:

`Active tail -> Transcript committed -> Native scrollback`

Rules:

1. Active pane rows are transient; do not directly commit them to native scrollback.
2. Only committed transcript overflow is inserted via `insert_before`.
3. Once committed to native scrollback, treat it as immutable history.
4. Overflow commits must be based on **commit capacity policy**, not blindly on current visual chat height.

## 9.1 Commit capacity vs visual capacity (important)

When an active pane is visible, chat has two relevant capacities:

- `visual_chat_capacity`: rows currently visible in chat area (reduced by active pane height)
- `commit_chat_capacity`: rows considered eligible before scrollback commit logic runs

Policy:

- For `SpillToTranscript` panes: `commit_chat_capacity` can match visual capacity.
- For `OccludeOnly` panes: `commit_chat_capacity` must ignore temporary occlusion (effectively treat active height as `0` for commit math).

Why:
- opening a big menu should visually push chat up,
- but this temporary push must **not** create irreversible native scrollback commits.

Practical consequence:
- while an occluding pane is open, transcript rows may be out of the inline viewport,
- but they stay in transcript state,
- and reappear when pane closes.

## 9.2 Scroll-disengage close behavior for occluding panes

For occluding panes, support a disengage gesture:

1. user scrolls upward while occluding pane is open
2. increment `scroll_up_disengage_count`
3. if count reaches threshold (e.g. `3`), close active pane
4. reset disengage counter
5. recompute layout; chat is pulled back down from canonical transcript

Keep in mind:
- reset disengage count on successful menu interaction, downward scroll, or pane switch
- do not mutate transcript or scrollback when auto-closing the pane
- this is a UI state transition only

Resize expectations:
- In-app active/transcript content can rewrap from canonical state.
- Native scrollback wrapping is terminal-managed and not reclaimed.

---

## 10) Step-by-step implementation phases

## Phase 0 — Terminal + exit foundation

- Introduce `InlineTerminal` wrapper as the terminal boundary for inline mode.
- Introduce `ExitState` and run a step/state-machine loop instead of post-loop shutdown cleanup.
- Keep shutdown as explicit phases (`Requested -> FinalizeActive -> FlushHistory -> FinalFrame -> Done`).
- Keep history insertion terminal-primitive based (`commit_rows`) and invalidate/repaint after insertions in flush phase.

## Phase 1 — Structural baseline

- Add active pane area to layout between chat and input.
- Active height: `0` when inactive, default `3` when active.
- Keep behavior unchanged otherwise.

## Phase 2 — Active state + lifecycle wiring

- Add active pane state and kinds.
- On submit: commit user msg + create `WorkingSpinner` active pane.
- Add placeholder transition to `AgentStreaming` when first assistant content arrives.

## Phase 3 — Canonical assistant streaming

- Store active canonical `full_text` + `frozen_until`.
- Implement chunk append + spill algorithm.
- Keep transcript canonical.

## Phase 4 — Streaming markdown

- On chunk: parse markdown from full assistant text and rebuild styled output.
- On tick: spinner only.
- Ensure active and transcript consume same styled line model.

## Phase 5 — Scrollback integration hardening

- Replace pending-vector-first flow with transcript uncommitted-row boundaries (`committed_rows` monotonic index).
- Commit via terminal primitive row insertion (`commit_rows` -> `insert_before`) rather than app-owned pending queues.
- During shutdown flush, insert in chunks and call redraw invalidation between chunks.
- Verify no duplicate/missing lines across active->transcript->scrollback transitions.

## Phase 6 — Resize hardening

- Recompute wraps on width change from canonical text.
- Preserve monotonic boundaries (`frozen_until`, transcript commit boundaries).
- Verify no regressions under frequent terminal resize.

## Phase 7 — Occluding pane policy + disengage

- Add explicit active-pane behavior policy (`SpillToTranscript` vs `OccludeOnly`).
- Implement commit-capacity logic so occluding pane height does not trigger scrollback commits.
- Add scroll-up disengage counter/threshold (e.g. close menu after 3 upward scroll intents).
- Verify closing occluding pane restores chat viewport from canonical transcript without duplication/loss.

---

## 11) Manual verification checklist

Run these manually after each phase:

1. Submit short message -> spinner appears below user message.
2. First delta switches spinner to streaming pane.
3. Long assistant output pushes stale lines from active into transcript continuously.
4. Active pane remains clamped; chat grows upward.
5. Overflow transcript commits into native scrollback without duplication.
6. Resize terminal during stream; no crashes, no reordered text.
7. Abort stream; partial output finalizes cleanly and active clears.
8. Spinner tick does not trigger markdown parse work.
9. Open large `/` or `@` menu pane; verify chat is pushed out visually but no new native scrollback commit happens because of menu height alone.
10. While menu is open, scroll up repeatedly; after threshold, menu closes and chat drops back down correctly.
11. Closing menu does not duplicate, lose, or reorder transcript lines.

---

## 12) Common pitfalls to avoid

- Storing only wrapped lines as source of truth.
- Mutating transcript formatting directly from spinner tick path.
- Committing transient active lines directly to scrollback.
- Letting visual occlusion from menu panes trigger scrollback commits.
- Treating visual chat height as commit capacity for all pane kinds.
- Mixing user-message rendering into active pane.
- Letting boundaries move backward.

---

## 13) Practical principle

Keep it simple and incremental:

- First correctness of boundaries and ownership.
- Then markdown quality/perf tuning.
- Then extra pane types and polish.

If invariants hold, the rest is mostly implementation detail.

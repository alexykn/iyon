Plan Addendum: Terminal-aware history + exit state machine

 ### Why this addendum is needed

 Your current architecture is strong for rendering, but shutdown/scrollback is still
 “app-vec drain first, terminal side effect second”.
 To make it robust (and compatible with active pane streaming), we need:

 1. Exit as a state machine (not post-loop cleanup)
 2. History insertion as a terminal primitive
 3. Transcript commit boundaries in row-space (not chat.lines.len() vs area height)

 ────────────────────────────────────────────────────────────────────────────────

 File-by-file target architecture (fits existing project)

 ### 1) crates/iyon-tui/src/core/state.rs

 Add:

 - exit_state: ExitState (replace/augment exit: bool)
 - transcript: TranscriptState (new; canonical + render cache + commit boundary)
 - active: Option<ActivePaneState> (as already planned)
 - keep input, info, scroll etc.

 New enums/structs:

 ```rust
   enum ExitState {
       Running,
       Requested,
       FinalizeActive,
       FlushHistory,
       FinalFrame,
       Done,
   }

   enum ActiveBehavior {
       SpillToTranscript,
       OccludeOnly,
   }
 ```

 TranscriptState (implemented shape):
 - canonical items
 - rendered rows cache for current width
 - `TranscriptCommitBoundary { logical_row, span_index, byte_offset }` monotonic boundary

 Note: older sketches used `committed_rows: usize`; the current implementation uses the richer commit boundary so resize/rewrap can preserve the committed frontier across soft wraps and styled spans.

 ────────────────────────────────────────────────────────────────────────────────

 ### 2) crates/iyon-tui/src/core/terminal.rs (new)

 Wrapper around DefaultTerminal:

 Responsibilities:
 - draw(...) + cache last frame area
 - insert_history_rows(&[Line])
 - expose last_viewport_area()
 - expose invalidate_next_draw() (force full redraw after direct history insertion)

 This gives you one place to handle inline viewport quirks and bookkeeping.

 ────────────────────────────────────────────────────────────────────────────────

 ### 3) crates/iyon-tui/src/core/scrollback.rs

 Refactor from commit_pending(Vec<Line>) to row-primitive API:

 ```rust
   commit_rows(terminal: &mut InlineTerminal, rows: &[Line<'static>]) ->
 Result<CommitResult>
 ```

 CommitResult:
 - rows_inserted
 - optional diagnostics (for debugging line loss)

 No app-owned pending queue as primary mechanism long-term.

 ────────────────────────────────────────────────────────────────────────────────

 ### 4) crates/iyon-tui/src/app.rs

 Move from:
 - while !exit { commit; draw; input } + finalize_exit()

 to:
 - loop { step(...) } with ExitState phases

 step() behavior:

 - Running:
     - process backend/input events
     - finalize active->transcript spill as needed
     - commit transcript overflow rows (policy-aware)
     - draw running frame
 - Requested -> FinalizeActive
 - FinalizeActive:
     - SpillToTranscript: freeze tail into transcript
     - OccludeOnly: drop active pane, no transcript mutation
     - transition FlushHistory
 - FlushHistory:
     - repeatedly commit transcript uncommitted rows in chunks
     - after each commit: terminal.invalidate_next_draw()
     - if empty: FinalFrame
 - FinalFrame:
     - draw chat tail + goodbye in input pane
     - one extra settle draw tick (optional but recommended)
     - Done

 ────────────────────────────────────────────────────────────────────────────────

 ### 5) crates/iyon-tui/src/core/layout.rs

 Extend computed layout to carry both capacities:

 - visual_chat_capacity_rows
 - commit_chat_capacity_rows

 Policy:
 - SpillToTranscript: commit capacity follows visual
 - OccludeOnly: commit capacity ignores active pane occlusion

 This directly matches your active-pane doc section 9.1.

 ────────────────────────────────────────────────────────────────────────────────

 ### 6) crates/iyon-tui/src/core/render.rs

 Keep renderer mostly as-is.
 Add explicit draw modes:

 - draw_running(...)
 - draw_exit(...)

 draw_exit should render goodbye in input area only (not transcript history).

 ────────────────────────────────────────────────────────────────────────────────

 Integration with ACTIVE_PANE_STREAMING_PLAN.md

 Add these as explicit phases in that doc:

 ### New Phase 0 — Terminal + exit foundation

 - introduce InlineTerminal wrapper
 - introduce ExitState
 - move shutdown into main state machine loop

 ### Phase 5 update — Scrollback integration hardening

 Replace “pending vector drain” language with:
 - transcript uncommitted row boundary
 - terminal primitive commit + redraw invalidation

 ### Phase 7 compatibility

 No changes in intent; now enforced by capacity split and behavior policy hooks.

 ────────────────────────────────────────────────────────────────────────────────

 Implementation order (safe, incremental)

 1. State machine skeleton in app.rs (no behavior change yet)
 2. InlineTerminal wrapper (delegate-only first)
 3. ExitState flow (Requested -> FinalFrame -> Done)
 4. Transcript committed boundary (keep existing chat lines temporarily as source)
 5. Overflow math switch to commit-capacity rows
 6. Active finalization in exit path
 7. Canonical transcript + render cache migration

 ────────────────────────────────────────────────────────────────────────────────

 Acceptance checks (must pass before moving on)

 1. Ctrl+C empty input -> goes through ExitState, not abrupt return
 2. Exit flush commits all transcript rows (count in/out matches)
 3. Goodbye appears in input pane in final frame
 4. No first-item-loss on first overflow commit
 5. Occluding pane open/close does not create irreversible scrollback commits
 6. Spill panes stream active->transcript->scrollback without duplication/loss

 ────────────────────────────────────────────────────────────────────────────────

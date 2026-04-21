# Inline + Scrollback Findings And Plan

This note summarizes what we verified from public source code and what we should build in `iyon`.

## Verified Findings

## 1) Codex is not `println!`-based for main chat UI
- Codex uses a real TUI stack (Ratatui + Crossterm) for live rendering.
- It keeps explicit viewport state and a terminal management layer.
- It supports inline/main-screen behavior where history remains in terminal scrollback.

## 2) Codex does not use an "infinite viewport"
- Viewport is bounded by terminal size.
- Older rendered content is moved/inserted above the live viewport into scrollback.
- This is managed explicitly, not by a stock auto-layout behavior.

## 3) Shared display model is the key
- Transcript units are represented as `HistoryCell` values.
- `HistoryCell` produces display lines (`Vec<Line<'static>>`) and desired heights.
- The same display-shaped content conceptually drives both:
  - live viewport rendering
  - committed history insertion

This is one proven approach. It is not the only possible one.

## 4) Input/editor and transcript are separate domains
- Codex separates chat composer/editor (`TextArea`, `ChatComposer`) from transcript cells (`HistoryCell`).
- Do not force one global trait across everything.

## 5) Robust key handling relies on normalization
- Multiple key combos map to same commands.
- Terminal variability is expected (especially on macOS).
- Portability comes from command mapping, not from one shortcut.

## 6) Pi also uses a custom terminal layer
- Pi (`pi-mono`) uses its own differential renderer and overlay composition.
- It renders components to line output and manages terminal updates itself.

## 7) OpenCode comparison
- We did not complete a source-level verification path in this session for the exact scrollback mechanics.

---

## What This Means For `iyon`

## Core Principle
Use a **single formatted render output** and fan out to both outputs:
1. live rendering in ratatui
2. scrollback commit path (inline mode)

Do **not** maintain two independent formatting systems.

## Canonical Pipeline (Aligned With `RENDER_ARCHITECTURE.md`)

1. `domain/backend events` -> semantic presentation items (`Presentable` / `TimelineItem`)
2. semantic items -> formatted styled lines (`Vec<Line<'static>>`)
3. frontend composition/event handling performs formatting from semantic items
4. same lines -> live render (`Paragraph/Text`)
5. same lines -> scrollback commit (serialize `Span::style` to terminal commands)

This keeps parity pressure in one place: the formatter.
Geometry (wrapping, viewport slicing, line-height) remains in renderer/commit stages.

## Upstream Construction Choices

### A) Direct to `Line/Span`
- Map semantic items directly to styled lines.
- Best default while architecture is still evolving.

### B) Semantic IR then lower to `Line/Span`
- Add richer intermediate structure only when needed (collapsible groups, deferred layout, richer metadata).
- Still lower to `Vec<Line<'static>>` before both sinks.

### C) ANSI-first upstream
- Keep ANSI as source format upstream if desired.
- Convert ANSI -> `Text/Line/Span` for live rendering (`ansi-to-tui`).
- For consistency with the shared pipeline, treat the parsed result as the canonical formatted lines for rendering + commit flow.

## Recommended Architecture

### Domain State
- `AppState`
  - `timeline: Vec<TimelineItem>`
  - `input_buffer: InputBuffer`
  - `ui_state: UiState` (`scroll`, `follow_bottom`, `focus`, etc.)

### Presentation Mapping
- `TimelineItem` / `Presentable` is the semantic layer.
- Formatter converts semantic items to `Vec<Line<'static>>`.
- Formatter stays width-agnostic.

### Live Renderer
- Ratatui consumes the formatted lines and renders visible viewport slice.
- Input rendering remains separate (`InputView`) and sets cursor position on `Frame`.
- Chat viewport should support a **transient active pane** directly above input (spinner/stream/tool UI), with transcript lines above it.

### Optional Inline Commit Layer (advanced)
- A small terminal manager that:
  - tracks live viewport rect
  - queues committed formatted lines
  - inserts committed lines above viewport into scrollback
- This is a separate concern from UI formatting.

---

## Trait Shape We Agreed On

Use typed views + centralized renderer:

```rust
pub trait View {}

pub trait Renderable<V: View> {
    fn render(&self, view: &V, frame: &mut ratatui::Frame, area: ratatui::layout::Rect);
}
```

Keep cursor in `InputView` only, not in a global view trait.

```rust
pub struct InputView<'a> {
    pub text: &'a str,
    pub cursor_bytes: usize,
}

pub struct ChatView<'a> {
    pub lines: &'a [ratatui::text::Line<'static>],
}
```

Use borrowed fields in frame-local view structs to reduce cloning and allocation churn during redraws.

---

## Implementation Plan (Ordered)

## Phase 1 (now): Stable fullscreen harness
1. Finish `InputBuffer` correctness (grapheme movement/deletion, word movement).
2. Normalize key events to internal commands (`InputCmd`).
3. Introduce typed timeline items (`User`, `Assistant`, `Tool`, `Diff`, `Status`).
4. Add semantic->formatted-lines formatter (`Vec<Line<'static>>`).
5. Render only visible viewport slice from timeline.
6. Keep input pinned at bottom, chat-pane internal scrolling.

## Phase 2: Shared formatted output hardening
1. Ensure all timeline rendering comes from formatted lines.
2. Remove duplicate formatting paths.
3. Add per-item style/render rules while preserving same line model.
4. If ANSI-first is used upstream, keep the conversion step centralized and deterministic.
5. Keep width/wrapping/viewport math out of formatter.
6. Introduce `ActivePane` model for transient bottom UI (`WorkingSpinner`, `StreamingAssistant`, later tool/diff panes).
7. Reserve pane height bottom-up; transcript uses remaining chat rows.

## Phase 3 (advanced): Inline + scrollback commit mode
1. Build terminal/viewport manager module.
2. Add queue of committed history lines.
3. Insert committed lines above live viewport.
4. Keep live viewport bounded to terminal height.
5. Commit only transcript overflow (not transient active pane rows).
6. On `TurnFinished`, finalize active pane into transcript, then normal overflow rules apply.

---

## Practical Guidance

- Build Phase 1 first. This already gives strong UX and lower complexity.
- Only add Phase 3 once timeline + viewport math are stable.
- Treat active panes as ephemeral view state; they should transition cleanly from spinner -> streaming -> committed transcript item.
- Avoid premature custom terminal machinery while input/render model is still changing.
- Do not let semantic classification logic leak into `Renderable`.

---

## Source Pointers

- Codex history cells:
  - https://github.com/openai/codex/blob/main/codex-rs/tui/src/history_cell.rs
- Codex input/composer:
  - https://github.com/openai/codex/blob/main/codex-rs/tui/src/bottom_pane/chat_composer.rs
  - https://github.com/openai/codex/blob/main/codex-rs/tui/src/bottom_pane/textarea.rs
- Pi custom TUI layer:
  - https://github.com/badlogic/pi-mono/blob/main/packages/tui/src/tui.ts
- ANSI parser for Ratatui:
  - https://docs.rs/ansi-to-tui/latest/ansi_to_tui/

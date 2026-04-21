# Render Architecture

This document captures the TUI render design for `iyon` and is the source of truth for render boundaries and contracts.

## Goals

- Keep domain/editor state separate from rendering.
- Use typed view structs (`InputView`, `ChatView`, etc.).
- Use one central renderer with per-view impls.
- Keep formatting parity between live UI and scrollback commit path.
- Avoid forcing unrelated fields into shared traits.
- Keep semantic presentation mapping separate from terminal paint calls.
- Keep formatting in the frontend composition layer (not backend transport/orchestration).

## Non-Goals

- No runtime backend switch inside `Renderable`.
- No formatting strategy decision inside `Renderable`.

## Mental Model

1. Event handlers mutate app/domain state.
2. Map domain events/state to semantic presentation items.
3. Format semantic items into shared render lines (`Vec<Line<'static>>`).
4. Build lightweight view structs from that formatted output.
5. Render live UI into ratatui areas.
6. Commit scrollback from the same formatted lines.

## Core Traits

```rust
use ratatui::{Frame, layout::Rect};

pub trait View {}

pub trait Renderable<V: View> {
    fn render(&self, view: &V, frame: &mut Frame, area: Rect);
}
```

Notes:

- `View` is intentionally minimal.
- `Renderable<V>` is for live `ratatui::Frame` rendering only.
- Scrollback output is a separate contract.

## Semantic Layer (Before Rendering)

```rust
pub enum Presentable {
    UserMessage { text: String },
    AssistantMessage { text: String },
    ToolCallStarted { name: String },
    ToolCallFinished { name: String, status: String },
    DiffChunk { header: String, lines: Vec<String> },
    StatusLine { text: String },
}
```

```rust
use ratatui::text::Line;

pub trait PresentableFormatter {
    fn format_presentable(&self, item: &Presentable) -> Vec<Line<'static>>;
}
```

Notes:

- This is the canonical presentation decision point.
- Rendering should not decide whether something is a tool call, diff, status, etc.

## View Types

```rust
use ratatui::text::Line;

#[derive(Debug, Clone)]
pub struct InputView<'a> {
    pub text: &'a str,
    pub cursor_bytes: usize,
}
impl View for InputView<'_> {}

#[derive(Debug, Clone)]
pub struct ChatView<'a> {
    // Already formatted for live rendering and scrollback parity.
    pub lines: &'a [Line<'static>],
}
impl View for ChatView<'_> {}

#[derive(Debug, Clone)]
pub struct InfoView<'a> {
    pub status: &'a str,
}
impl View for InfoView<'_> {}
```

Important:

- Cursor is only in `InputView`.
- Other views do not carry cursor fields.
- `ChatView` carries already-formatted lines, not semantic/domain state.
- Prefer borrowed view fields (`&str`, `&[Line]`) in the draw path to avoid clone-heavy per-frame allocations.

## Canonical Presentation Construction

Pick one approach in composition/wiring, but both should converge to shared styled lines.

### Option A: Direct formatter to `Line/Span`

- Format semantic items straight to `Vec<Line<'static>>`.
- Live path renders those lines.
- Commit path serializes those same lines to terminal style commands/ANSI.

### Option B: IR-first canonical builder

- Build richer semantic IR first when needed (collapsible groups, structured diff metadata, etc.).
- Lower IR to `Vec<Line<'static>>`.
- Use the same lowered lines for live and scrollback.

```rust
pub trait PaneIrBuilder<V: View, I> {
    fn build_ir(&self, view: &V) -> I;
}

pub trait IrToLines<I> {
    fn to_lines(&self, ir: &I) -> Vec<ratatui::text::Line<'static>>;
}
```

Constraint:

- Do not branch between these options inside `Renderable`.
- `Renderable` paints preformatted view data only.

## Central Renderer

```rust
#[derive(Debug, Default)]
pub struct Renderer;
```

### Input Rendering (cursor placement)

```rust
use ratatui::widgets::{Block, Borders, Paragraph};
use unicode_width::UnicodeWidthStr;

impl Renderable<InputView> for Renderer {
    fn render(&self, view: &InputView, frame: &mut Frame, area: Rect) {
        let widget = Paragraph::new(view.text.as_str())
            .block(Block::new().borders(Borders::ALL));
        frame.render_widget(widget, area);

        // Visual cursor column is display width, not byte index.
        let prefix = view.text.get(..view.cursor_bytes).unwrap_or(view.text.as_str());
        let max_cols = area.width.saturating_sub(2) as usize;
        let column = prefix.width().min(max_cols);

        let x = area.x + 1 + u16::try_from(column).unwrap_or(u16::MAX);
        let y = area.y + 1;
        frame.set_cursor_position((x, y));
    }
}
```

### Chat Rendering

```rust
impl Renderable<ChatView> for Renderer {
    fn render(&self, view: &ChatView, frame: &mut Frame, area: Rect) {
        let text = ratatui::text::Text::from(view.lines.clone());
        frame.render_widget(ratatui::widgets::Paragraph::new(text), area);
    }
}
```

### Info Rendering

```rust
impl Renderable<InfoView> for Renderer {
    fn render(&self, view: &InfoView, frame: &mut Frame, area: Rect) {
        frame.render_widget(
            ratatui::widgets::Paragraph::new(view.status.as_str())
                .block(ratatui::widgets::Block::new().borders(ratatui::widgets::Borders::ALL)),
            area,
        );
    }
}
```

## Active Pane Model (Transient Bottom UI)

To support UX like "show a `Working...` spinner immediately after user submit, then replace it with streaming assistant text on first token", model chat as two layers:

1. **Committed transcript** (scrollback-eligible)
   - Immutable-ish timeline items that have completed.
   - Source for overflow -> inline scrollback commit.
2. **Active panes** (ephemeral, bottom-anchored)
   - Live-updating pane instances rendered above input.
   - Examples: `WorkingSpinner`, `StreamingAssistant`, `ToolApproval`, `DiffPreview`.

Rules:
- Active panes are rendered first (bottom reservation), transcript fills remaining chat rows.
- Active panes are not committed directly to scrollback while transient.
- On completion, pane output is converted to a normal transcript item and then follows normal overflow commit rules.
- The same canonical `Vec<Line<'static>>` formatting is still required for parity.

Suggested shape (start with enum, add trait abstraction only if needed):

```rust
pub enum ActivePane {
    WorkingSpinner { frame: u64 },
    StreamingAssistant { lines: Vec<Line<'static>> },
    // later: ToolApproval, DiffPreview, ...
}

impl ActivePane {
    pub fn desired_height(&self, width: u16) -> u16 { /* ... */ }
    pub fn render_lines(&self, width: u16) -> Vec<Line<'static>> { /* ... */ }
}
```

Transition intent:
- `UserSubmitted` -> create `WorkingSpinner` pane (3-line box visual).
- `AssistantDelta(first token)` -> replace same pane with `StreamingAssistant`.
- `AssistantDelta(next)` -> update pane content in place.
- `TurnFinished` -> finalize pane into transcript item; clear active pane.
- `TurnFailed` -> replace/finalize as error/status item.

## Scrollback Contract (Separate From `Renderable`)

```rust
pub trait ScrollbackCommit {
    fn commit_lines(&mut self, lines: &[ratatui::text::Line<'static>]);
}
```

Notes:

- `Renderable` does not emit scrollback.
- Scrollback serialization reuses the same `Line/Span` style information used by live rendering.
- Width, wrapping, viewport slicing, and line-height decisions belong to renderer/commit stages.

## App Draw Wiring

```rust
fn draw(&self, frame: &mut Frame, state: &AppState) {
    let r = self.layout.compute(frame.area());

    let input_view = InputView {
        text: state.input.text().to_string(),
        cursor_bytes: state.input.cursor_bytes(),
    };

    let chat_view = ChatView {
        lines: state.chat_lines.clone(), // Prepared upstream.
    };

    let info_view = InfoView {
        status: state.status_line.clone(),
    };

    self.renderer.render(&chat_view, frame, r.chat_area);
    self.renderer.render(&input_view, frame, r.input_area);
    self.renderer.render(&info_view, frame, r.info_area);
}
```

## Composition Boundary (Where strategy is chosen)

Strategy decision belongs in wiring/composition, not renderer:

```rust
// Pseudocode
// let presentables = mapper.map_timeline(&domain_events);
// let lines = formatter.format_all(&presentables);
// let chat_view = ChatView { lines: lines.clone() };
// renderer.render(&chat_view, frame, area);
// scrollback.commit_lines(&lines);
```

## InputBuffer Contract

`InputBuffer` should expose enough read-only state for `InputView` construction:

```rust
impl InputBuffer {
    pub fn text(&self) -> &str { /* ... */ }
    pub fn cursor_bytes(&self) -> usize { /* ... */ }
}
```

## Why This Structure

- Strong typing per view.
- Renderer stays focused on painting.
- Formatting parity is enforced by one canonical source per pane/path.
- No hidden mode-switching in renderer.
- Input/editor logic remains clean.

## Expansion Path

1. Add `TimelineItem` and build `ChatView` from typed items.
2. Add viewport/scroll state in `UiState`.
3. Add `DiffView` / `ToolCallView` with dedicated `Renderable` impls.
4. Keep pipeline stable: `AppState -> View -> canonical presentation -> render/commit`.

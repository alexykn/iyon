# Iyon Architecture

> Three layers of abstraction, each with a different audience and contract, plus
> a cross-cutting plugin system that lets every layer be extended at runtime.

---

## Overview

```
iyon-tui            — framework (owned by iyon-tui maintainers)
iyon-components     — reusable widget library (owned by iyon-components maintainers)
iyon/               — application (owned by Iyon product)
iyon-plugins/       — WASM plugin wiring (owned by plugin system maintainers)
```

Each layer builds on the one below it using only its **public API**. There is no
special access to internals. If something cannot be built cleanly on the public
API, that is a signal that the lower layer is missing a primitive.

Plugins extend any layer at runtime via WASM + WIT, without recompiling the host.

---

## Layer 1: `iyon-tui` — The TUI framework

**Audience:** Application authors who need a streaming-first, semantic terminal UI.

`iyon-tui` is a generic Rust TUI framework. It owns:

| Domain | What |
|---|---|
| **`View`** | Owned backend-neutral composition tree — rows, columns, grids, hanging, containers, text, spacer, clamp, component slots |
| **`Component`** | Retained-state widget contract — `view()` + `capabilities()` |
| **`App`** | Application runtime — `init` / `update` / `view` lifecycle, key dispatch, paste, timers, external actions |
| **`History`** | Ordered scrollback with frozen units, live streams, flow boundaries, native terminal transfer |
| **`StreamPane` / `TextStream`** | Live streaming source protocol with incremental compilation and viewport anchoring |
| **`TextContent` / `TextRenderer`** | Semantic text IR (blocks, inlines, marks), Markdown/plain projectors, `Renderer` trait |
| **`Theme`** | Named colors, style specs, variant selectors (focus, state key/value) |
| **`TextInput`** | Single built-in control (multiline, paste, cursor, submission output) |
| **`ScrollPane`** | Generic scrollable container for frozen content |
| **`DiffRenderer`** | Structured diff → View |
| **`Projection` / `Projector` / `Smooth`** | Incremental source-mapped transformation pipeline |
| **`Output` / `OutputRouter` / `EventCx`** | Typed event emission and routing |
| **Testing** | Headless `AppHarness`, `compile_view_lines`, `style_at_text`, `cell_x_of_text` |

**What it does NOT own:** agents, tools, providers, conversation UI, status bars,
command palettes, menus, modals, tree views, tab bars, or any application-specific
widget. Those belong in the layers above.

**Plugin boundary:** `iyon-tui` is not plugin-aware itself. Plugin support for the
View pipeline — custom renderers, projectors, menus, keybindings, themed styles,
status bar widgets — lives in `iyon-tui/plugins/` as a WIT-defined sub-crate (see
[Plugin Architecture](#plugin-architecture) below). The framework crate itself has
no WASM dependency.

---

## Layer 2: `iyon-components` — Reusable generic widgets

**Audience:** Any application built on `iyon-tui` that wants pre-built interactive
widgets without writing them from scratch.

`iyon-components` provides **behavior + default look** for common TUI interaction
patterns. Every widget is a trait that defines what the widget does, with a
default `render()` that produces a `View`. Consumers can override `render()` to
supply their own layout while keeping all the behavior (keyboard navigation,
focus management, output routing) for free.

### Design principle: trait contract with `render()` override

```rust
// iyon-components defines the contract
pub trait ListSelector: Sized + 'static {
    type Item: Clone;

    fn items(&self) -> &[Self::Item];
    fn selected_index(&self) -> usize;
    fn label(&self, index: usize) -> &str;

    // Behavior — public so consumers can call them, but default impls exist
    fn select_next(&mut self);
    fn select_previous(&mut self);
    fn select_index(&mut self, index: usize);

    // Output — consumer routes this to their action enum
    fn selection(&self) -> Output<(usize, Self::Item)>;

    // ESCAPE HATCH: override to supply your own layout
    fn render(&self) -> View {
        // Default: vertical list with arrow highlight
        // Consumers replace this entirely without losing behavior
        View::vertical(|col| {
            for i in 0..self.items().len() {
                let is_selected = i == self.selected_index();
                let prefix = if is_selected { "▸ " } else { "  " };
                let style = if is_selected { highlighted() } else { normal() };
                col.child(View::text(format!("{}{}", prefix, self.label(i))).style(style));
            }
        })
    }
}

// Blanket-impl Component so consumers just impl the trait
impl<T: ListSelector + 'static> Component for T {
    fn view(&self) -> View { self.render() }
    fn capabilities(&self, cx: &mut ComponentCx<'_, Self>) {
        cx.focusable();
        cx.bind_key(Key::Down, |this| this.select_next());
        cx.bind_key(Key::Up, |this| this.select_previous());
        cx.bind_key(Key::Enter, |this| { /* emit selection output */ });
    }
}
```

Consumer usage:

```rust
#[derive(Default)]
struct ModelPicker {
    selected: usize,
    selection: Output<(usize, Model)>,
}

impl ListSelector for ModelPicker {
    type Item = Model;
    fn items(&self) -> &[Model] { &MODELS }
    fn selected_index(&self) -> usize { self.selected }
    fn label(&self, i: usize) -> &str { MODELS[i].name }
    fn select_next(&mut self) { self.selected = (self.selected + 1).min(MODELS.len() - 1); }
    fn select_previous(&mut self) { self.selected = self.selected.saturating_sub(1); }
    fn select_index(&mut self, i: usize) { self.selected = i.min(MODELS.len() - 1); }
    fn selection(&self) -> Output<(usize, Model)> { self.selection }
    // Uses default render() — gets vertical arrow list for free
}

// Or override render() for a compact horizontal pill layout:
fn render(&self) -> View {
    View::horizontal(|row| {
        for i in 0..self.items().len() {
            let pill = View::text(format!(" {} ", self.label(i)))
                .border(if i == self.selected_index() {
                    BorderSpec::plain().color(ColorSpec::theme("accent"))
                } else {
                    BorderSpec::plain()
                });
            row.child(pill);
        }
    })
    // select_next/previous, focus, Enter → output all still work
}
```

### Planned widgets

| Widget | Contract Trait | Default behavior |
|---|---|---|
| **`ListSelector`** | arrow/highlight navigation, selection output | Vertical list with `▸` marker |
| **`TabBar`** | active tab tracking, next/prev/select-index | Horizontal row, active underlined |
| **`Dialog`** | confirm/cancel outputs, focus trap | Bordered box, title, action buttons |
| **`CommandPalette`** | filtered list, text input, selection output | TextInput + filtered ListSelector |
| **`TreeView`** | expand/collapse, depth indent, selection | Collapsible tree with `▶`/`▼` |
| **`ProgressBar`** | determinate/indeterminate, label | `[████····] 60%` |
| **`Spinner`** | frame animation, label | `⠋ Working` |
| **`Table`** | sortable columns, pagination, zebra | Grid with header, row selection |
| **`SplitPane`** | resizable divider, ratio tracking | Vertical/horizontal split with drag handle |
| **`Accordion`** | expand/collapse sections | Stacked collapsible panels |
| **`Badge` / `KeyHint`** | label with style | `[3]` / `Ctrl+C` |
| **`StatusBar`** | left/right sections, dynamic content | Bottom bar with segments |
| **`Breadcrumbs`** | path segments with separators | `Root › src › main.rs` |

### What `iyon-components` does NOT own

- **No agent concepts** — no tools, no providers, no conversation, no streaming
  text pipeline. Those are Iyon's job.
- **No application state** — widgets receive data, they don't fetch it.
- **No plugin system** — components are compiled-in Rust. Plugin support for
  UI extensions (menus, keybindings, styles) is `iyon-tui/plugins/`, which is
  a different system for a different purpose (runtime-loaded WASM capabilities
  vs. compile-time widget reuse).

### Relationship to plugins

Plugins and components are orthogonal:

- **Plugins** extend `iyon-tui`'s *capabilities at runtime* — new model providers,
  tool implementations, custom renderers/projectors. These are loaded from `.wasm`
  files and use WIT interfaces.
- **Components** extend what you can *build at compile time* — reusable widgets
  that save you from writing the same keyboard navigation, focus handling, and
  output routing for the hundredth time.

A plugin author might use `iyon-components` internally to build their plugin's
UI. A component consumer might load their own plugins at runtime. Neither depends
on the other.

### All `iyon-components` needs

```toml
[dependencies]
iyon-tui.workspace = true
serde = { workspace = true, optional = true }
```

That's it. No tokio, no crossterm, no pulldown-cmark, no reqwest. Pure `iyon-tui`
public API. If a component needs persistence or serialization, it's optional.

---

## Layer 3: `iyon/` — The Iyon agent application

**Audience:** End users running an AI coding agent in their terminal.

`iyon/` is the application. It owns:

- **Provider integration** — OpenRouter, OpenAI Codex, Mock. These call `iyon-api`.
- **Core agent loop** — turn management, tool execution, approval flow. These call `iyon-core`.
- **Conversation UI** — `ConversationActivity`, `UserBatch`, steering queue, tool
  call cards, streaming assistant message pipeline. These build on `iyon-tui` and
  `iyon-components`.
- **Tool renderers** — bash, edit, read, write, grep, find, ls. These produce
  semantic `View` values from tool call/result data. Currently compiled-in;
  planned to migrate to WASM plugins under the `iyon-core/plugins/` WIT interface
  (see [Plugin Architecture](#plugin-architecture) below).
- **Assistant text pipeline** — Markdown projection, thinking segment tagging,
  pipe table detection, pacing atoms. This is Iyon-specific and lives in
  `iyon/src/tui/transcript/`.
- **The Iyon theme** — the color palette and text styles that give Iyon its look.
- **Auth** — credential management for OpenRouter, OpenAI Codex.
- **CLI** — `iyon run`, `iyon auth login`, etc.

### When tools become plugins

The current `iyon/src/tui/tools/` (handler+renderer for each built-in tool) is a
legacy of the pre-plugin architecture. These will migrate to individual `.wasm`
files:

```
Before:  handler in iyon-core/src/tools/builtin/
         renderer in iyon/src/tui/tools/renderers/
         (separate, manually kept in sync)

After:   bash.wasm — both handler and renderer in one WIT-bound unit
         edit.wasm — ditto
         read.wasm — ditto
         write.wasm — ditto
         grep.wasm — ditto
         find.wasm — ditto
         ls.wasm — ditto
         generic.wasm — fallback renderer for unregistered tools
         iyon-plugins/ loads them all at startup
```

The conversation UI (`ConversationActivity`, `TuiFormatter`, `TimelineItem`) stays
in `iyon/` because it's application-specific — it knows about Iyon's tool model,
approval flow, and streaming pipeline. It is not generic enough for
`iyon-components`, and it doesn't need to be a plugin.

---

## How the layers connect

```
iyon/ (application)
  │
  ├── uses iyon-api, iyon-core (agent runtime)
  ├── uses iyon-components (ListSelector for model picker, Dialog for approvals)
  └── uses iyon-tui (View, Component, App, History, TextInput, Theme)
          │
          └── optionally loads plugins via iyon-plugins/ (WASM at runtime)

iyon-components (widget library)
  └── uses iyon-tui public API only

iyon-tui (framework)
  └── no dependencies on iyon-components, iyon-core, or iyon-api
```

Each layer is independently testable, independently releasable, and independently
replaceable. You could use `iyon-tui` without `iyon-components`. You could use
`iyon-components` on top of `iyon-tui` for your own non-agent application. You
could run Iyon with custom components that you wrote yourself.

---

## Summary table

| Layer | What | Who builds it | Who consumes it |
|---|---|---|---|
| `iyon-tui` | TUI framework (View, Component, App, History, Stream, TextInput, Theme) | framework maintainers | Any TUI app author |
| `iyon-components` | Reusable widgets (ListSelector, TabBar, Dialog, TreeView, Table, ProgressBar, etc.) | component maintainers | Any `iyon-tui` app |
| `iyon/` | AI agent application (tools, providers, conversation UI, CLI, auth) | Iyon maintainers | End users |
| `iyon-plugins/` | WASM plugin wiring (discovery, registry, lifecycle) | plugin system maintainers | Iyon + plugin authors |

---

# Plugin Architecture

> Captured 15 August 2026 — design discussion covering the cross-cutting plugin
> system that lets every layer be extended at runtime via WASM + WIT.

---

## The Goal

Allow third-party developers to extend Iyon with:

1. **Model providers** — new LLM backends (e.g. Anthropic, Google, local Ollama)
2. **Tool implementations** — new capabilities (e.g. `docker`, `kubectl`, `db`, `weather`, `github`)
3. **Tool renderers** — custom display for those tools (tables, colored output, progress, spinners, etc.)
4. **Custom renderers / projectors** — new input format renderers (HTML, LaTeX, syntax highlighting, etc.)
5. **UI extensions** — menus, keybindings, themed styles, status bar widgets

All at runtime, post-compile, without recompiling Iyon, `iyon-core`, `iyon-tui`, or `iyon-api`.

---

## Guiding principles

- The plugin system must **not** compromise Iyon's streaming, History, presentation, or native scroll architecture.
- The plugin boundary should **clean the architecture**, not muddy it — built-in providers, tools, and renderers should eventually use the same plugin interface as third-party ones.
- A tool plugin defines both its **handler** and its **renderer** together, in one plugin unit. The current split (handler in `iyon-core`, renderer in `iyon/`) is a legacy of the pre-plugin architecture.
- `iyon-tui` is a generic Rust TUI toolkit. It should **not** become WASM-aware or plugin-aware directly — its plugin support lives in `iyon-tui/plugins/` as a WIT-defined sub-crate.
- `iyon-core` is a generic agent runtime. Its plugin support lives in `iyon-core/plugins/`.
- `iyon-api` defines the provider interface. Its plugin support lives in `iyon-api/plugins/`.
- `iyon-plugins/` is the **wiring crate** that ties all three sub-plugin systems together, handles discovery, registry, cross-crate sync, and lifecycle.
- **Dogfood principle:** If the WIT interface is ergonomic enough for the built-in tools, providers, and renderers, it's ergonomic enough for third-party plugins. Discomfort during built-in migration is a signal to fix the contract before releasing it publicly.

---

## Why WASM via WIT

**WASM** was chosen over:

| Approach | Rejected because |
|---|---|
| **Rhai** (embedded scripting) | Plugin authors are locked into Rhai; real I/O requires registering every host function; Views from Rhai are awkward |
| **Stdio/MCP protocol** | Slower (serialization + subprocess overhead per call), harder to distribute as single file, language-agnostic but leaky |
| **Dynamic linking (`dylib`)** | Platform-specific, unsafe, Rust ABI instability |
| **Custom C ABI** | Hand-maintaining a shared-memory ABI is unnecessary; WIT toolchain generates it |

WASM gives:

- **Language freedom** — plugins in Rust, Go, C, Zig, AssemblyScript, TinyGo, Python (via wasmtime-py), etc.
- **Sandboxing** — no arbitrary memory access, no unsafe Rust risk
- **Single file distribution** — one `.wasm` file per plugin
- **Performance** — near-native execution speed via wasmtime/wasmer
- **Deterministic interface** — WIT contract is the explicit API boundary
- **Generated bindings** — `wit-bindgen` handles the C ABI / shared memory details; we don't maintain them by hand

---

## Crate layout

```
iyon-api/plugins/          — WIT interface for model providers
iyon-core/plugins/         — WIT interface for tools, agent hooks
iyon-tui/plugins/          — WIT interface for rendering (hot path)
iyon-plugins/              — wiring: discovery, registry, lifecycle, cross-crate sync
```

### Why a sub-crate per library instead of one big plugin crate?

Each library knows its own IR best:

| Path | What it defines | ABI style |
|---|---|---|
| `iyon-api/plugins/` | Model provider interface | WIT (request/response, not hot path) |
| `iyon-core/plugins/` | Tool handler, agent hooks | WIT (async dispatch, not hot path) |
| `iyon-tui/plugins/` | View IR, renderer, projector | WIT (wit-bindgen generates efficient flat structs) |
| `iyon-plugins/` | Registry, discovery, wiring | N/A — orchestrates the above three |

The WIT toolchain handles the actual C ABI and shared memory layout. We don't hand-write any of it.

### Independence

Each sub-plugin module works standalone. If someone uses only `iyon-tui` without core or API, they get `iyon-tui/plugins/` by itself. If they want the full agent harness, `iyon-plugins/` loads everything and keeps the registries in sync.

---

## What plugins can do

A plugin can contribute any combination of:

| Capability | WIT interface | Lives in |
|---|---|---|
| Model provider | `model-provider` | `iyon-api/plugins/` |
| Tool handler + renderer | `tool` | `iyon-core/plugins/` + `iyon-tui/plugins/` |
| Custom projector | `projector` | `iyon-tui/plugins/` |
| Custom renderer | `renderer` | `iyon-tui/plugins/` |
| Menu items | `menu` | `iyon-tui/plugins/` |
| Keybindings | `keybinding` | `iyon-tui/plugins/` |
| Theme styles | `style` | `iyon-tui/plugins/` |
| Status bar widgets | `status-widget` | `iyon-tui/plugins/` |
| Agent lifecycle hooks | `hook` | `iyon-core/plugins/` |

A single `.wasm` file can declare multiple capabilities. The plugin entry point registers everything it provides:

```wit
interface plugin {
    /// Declare all capabilities this plugin provides
    register: func() -> list<capability>;
}
```

### The WIT contract (sketch per domain)

#### `iyon-api/plugins/` — model providers

```wit
interface model-provider {
    /// Stream a model request
    stream: func(request: model-request) -> stream<model-event>;

    /// List available models for this provider
    list-models: func() -> list<model-info>;
}

record model-request {
    system-prompt: option<string>,
    messages: list<message>,
    tools: list<tool-spec>,
    params: model-params,
}

// ... message types mirroring iyon-api's ModelMessage, ContentBlock, etc.
```

#### `iyon-core/plugins/` — tools and agent hooks

```wit
interface tool {
    /// List tools this plugin provides
    list-tools: func() -> list<tool-spec>;

    /// Execute a tool call
    execute: func(name: string, args: json) -> stream<tool-event>;
}

interface hook {
    /// Register lifecycle hooks
    list-hooks: func() -> list<hook-spec>;

    /// Handle a hook event
    handle-hook: func(event: hook-event) -> hook-result;
}
```

#### `iyon-tui/plugins/` — rendering and UI

This is the most performance-sensitive interface. Plugins produce the same IR that the Rust public API lowers to — `View`, `TextContent`, `Block`, `Inline` — just expressed as WIT data types instead of Rust structs.

```wit
interface renderer {
    /// Render a tool call display
    render-call: func(input: call-state) -> view;

    /// Render a tool result display
    render-result: func(input: result-state) -> view;
}

interface projector {
    /// Project raw text to semantic IR
    project: func(input: string) -> text-content;
}

/// The View IR — mirrors iyon_tui::View as WIT data
variant view {
    text(spans: list<text-span>),
    horizontal(children: list<view>, gap: option<u16>),
    vertical(children: list<view>, gap: option<u16>),
    grid(columns: list<track>, rows: list<track>, cells: list<grid-cell>),
    hanging(prefix: view, continuation: view, body: view),
    spacer(rows: u16),
    clamp-rows(child: view, max-rows: u16, overflow: overflow-indicator),
    styled(child: view, style: style-spec),
    container(child: view),
}

record text-span {
    text: string,
    style: style-spec,
}

record style-spec {
    bold: option<bool>,
    dim: option<bool>,
    italic: option<bool>,
    underline: option<bool>,
    strikethrough: option<bool>,
    foreground: option<color>,
    background: option<color>,
}

variant color {
    ansi(u8),
    rgb(u8, u8, u8),
    theme(string),
}

// Semantic text IR for projectors
variant text-content {
    block(block),
    raw(string),
}

variant block {
    paragraph(inline-content),
    heading(level: u8, content: inline-content),
    block-quote(blocks: list<block>),
    list(marker: list-marker, items: list<list-item>),
    code-block(language: option<string>, body: string),
    table(columns: list<table-column>, rows: list<table-row>),
    thematic-break,
}
```

### Lowering

The WIT `view` type is lowered into `iyon_tui::View` by code in `iyon-plugins/`. This is a thin, mechanical conversion — no layout, no measurement, just data shape translation.

```
WASM plugin produces WIT view
  → wasmtime host reads WIT data
  → iyon-plugins lowers WIT view → iyon_tui::View
  → standard View pipeline (layout, theme, paint)
```

The same engine handles both Rust-native `View` and plugin-produced `View`. No duplication. No special-casing.

### Language-specific bindings for plugin authors

Language freedom means every language that compiles to WASM can produce the
same WIT IR, but the ergonomics vary:

| Language | What the plugin author gets |
|---|---|
| **Rust** | `iyon-tui` + `iyon-components` at compile time. Same View API as the host, lowered to WIT by `wit-bindgen`. |
| **Zig, Go, C, TinyGo** | Hand-roll WIT records. These languages have `wit-bindgen` bindings but no widget library — authors build View IR directly. |
| **JavaScript, TypeScript** | A small companion library that mirrors the Rust public API shape and lowers it to WIT. No host involvement, no runtime cost. |

All three paths produce the same WIT `view` data on the wire. The host never
knows which path was taken.

#### JS/TS library sketch

```js
// iyon-view — a tiny JS/TS library compiled into the plugin bundle
// It mirrors iyon_tui::View construction and lowers to WIT records.

import { View, Text, horizontal, vertical, hanging, grid, spacer } from "iyon-view";

function renderToolCall(input) {
  return vertical(col => {
    col.child(Text(`$ ${input.command}`).bold());
    col.child(Text(input.result).foreground({ theme: "text.muted" }));
  });
}
```

The library is pure JS — no DOM, no host calls, no WASM dependency inside the
plugin. It exists only to save plugin authors from writing WIT records by hand.
The output is the same `view` variant either way.

**This is explicitly future work.** The priority is stabilizing the WIT contract
through the dogfood loop (migrating built-in tools → WASM). Language bindings
come after the contract is proven, starting with Rust plugin authors first.

---

## Plugin lifecycle

```
Startup:
  1. iyon-plugins scans plugin directories (~/.iyon/plugins/, embedded/)
  2. For each .wasm:
     a. Instantiate WASM module via wasmtime
     b. Call register() → get list of capabilities
     c. Route capabilities to sub-registries:
        - tool handler → iyon-core/plugins/ registry
        - renderer → iyon-tui/plugins/ registry
        - model provider → iyon-api/plugins/ registry
        - menu / keybinding / style → iyon-tui/plugins/ registry
     d. Store capability-to-plugin mapping
  3. Start application loop with loaded registries

Runtime (tool call example):
  1. Agent emits tool call → iyon-core dispatches to registered handler
  2. Handler WASM instance receives execute(name, args)
  3. Handler streams tool events back to core
  4. Core forwards events to TUI
  5. TUI dispatches render-call / render-result to the same plugin's renderer
  6. Renderer produces WIT view → lowered to iyon_tui::View → painted

Shutdown:
  1. Drop all WASM instances
  2. Clear registries
```

### Shared vs per-call instances

A plugin instance persists across calls within a session. Stateful plugins (e.g., holding a database connection) can initialize on `register()` and clean up on drop. The host provides an async bridge for plugins that need to perform I/O.

---

## Built-in components as plugins

The current compiled-in components should eventually migrate to the plugin interface.

### Providers

```rust
// crates/iyon-providers/openai.wasm       — OpenAICodexModelApi
// crates/iyon-providers/openrouter.wasm    — OpenRouterModelApi
// crates/iyon-providers/mock.wasm          — MockModelApi (testing)
```

### Tools

```rust
// crates/iyon-tools/bash.wasm    — bash tool handler + renderer
// crates/iyon-tools/read.wasm    — read tool handler + renderer
// crates/iyon-tools/write.wasm   — write tool handler + renderer
// crates/iyon-tools/edit.wasm    — edit tool handler + renderer
// crates/iyon-tools/grep.wasm    — grep tool handler + renderer
// crates/iyon-tools/find.wasm    — find tool handler + renderer
// crates/iyon-tools/ls.wasm      — ls tool handler + renderer
// crates/iyon-tools/generic.wasm — fallback renderer for unregistered tools
```

Each `.wasm` defines both the tool's **execution logic** and its **renderer** in one unit. The current split (handler in `iyon-core`, renderer in `iyon/tui/tools/renderers`) is a legacy that should be unified.

---

## Related work: OpenTUI

[OpenTUI](https://github.com/anomalyco/opentui) is a terminal UI toolkit with a
similar shape: a native engine (Zig) exposing a C ABI, with first-class TypeScript
bindings, React/Solid reconcilers, and Yoga (flexbox) layout. It powers
[OpenCode](https://opencode.ai) in production.

Relevant observations:

- **C ABI boundary works.** A Zig core can serve JS, React, and native consumers
  through the same ABI. This validates the WIT approach even though we use WASM
  instead of dynamic linking.
- **Hot path is native.** OpenTUI's measurement callbacks run in Zig even when
  the renderable tree lives in JS. The symmetric WIT split achieves the same
  separation: the public API (Rust or JS) produces WIT data, the engine consumes
  it. The high-level language never touches layout/paint.
- **Reconcilers are the component library.** OpenTUI doesn't bundle tab bars,
  list selectors, or dialogs in the core. Those live in userland or in
  framework-specific packages (Solid, React). This matches the separation
  between `iyon-tui` (framework) and `iyon-components` (widgets) — components
  sit on top of the public API, not inside the engine.
- **Yoga layout means flexbox.** OpenTUI doesn't build custom track sizing.
  They get `flexDirection`, `flexGrow`, `gap`, `alignItems` for free from Yoga.
  iyon-tui's current hand-rolled layout (measure/prepare/place, TrackSize enums,
  WidthRule, HeightRule) solves a similar problem but with more specialized
  primitives and less flexibility.

The main difference in direction: OpenTUI is **Zig → C ABI → TS/React**,
while Iyon is **Rust → WIT → any language**. Same layered shape, different
ABI choice, different target audience for the language bindings.

---

## Layout: eventual Taffy adoption

Today iyon-tui has a custom layout pipeline (`presentation/layout/`) with its
own track sizing (`TrackSize::Content/Fixed/Flex/FlexMax`, `WidthRule::Fill/Fit`,
`HeightRule::Fill/Fit`), gap management, and alignment. This is ~1.5 KLOC of
hand-rolled measurement, preparation, and placement.

**Taffy** is a Rust flexbox/grid library (used by Dioxus, Bevy, Slint, etc.)
that handles all of the above and more: flexbox, grid, min/max constraints,
cross-axis alignment, and percentage-based sizing. Adopting Taffy would replace
the custom layout engine with a well-tested, community-maintained one.

### What stays the same

The public API for constructing Views does not change:

```rust
View::horizontal(|row| {
    row.child("fixed item");
    row.flex(View::text("growing item").fill_width());
    row.gap(1);
});

View::vertical(|col| {
    col.child("top");
    col.flex(View::text("middle").fill_height());
    col.child("bottom");
});

View::grid(|grid| {
    grid.row(|row| {
        row.cell("A");
        row.cell("B");
    });
});
```

The typed builders (`Horizontal`, `Vertical`, `Grid`) continue to work as they
do today. The fluent chain (`View::text("x").fill_width().padding(1)`) is
unchanged. `View::hanging()` and `View::component()` are unaffected — they are
semantic primitives that layout engines handle as atomic children.

`clamp_rows` and `OverflowIndicator` stay custom (Taffy doesn't do truncation).
`RowViewport` stays custom (Taffy doesn't do semantic viewport clipping).

### What changes internally

- `View` construction lowers into Taffy `Style` structs instead of custom
  `TrackSize` enums
- `WidthRule::Fill` → `flex_grow: 1.0`, `WidthRule::Fit` → `flex_grow: 0.0`
- `TrackSize::Content { max }` → `min_content` / `max_content`
- `TrackSize::Fixed(n)` → exact size
- `TrackSize::Flex { min }` → `flex_grow: 1.0, min_size`
- Grid maps to Taffy's grid track and cell placement
- Layout becomes a Taffy compute pass + custom passes for hanging, clamp,
  viewport, and component slot resolution

### Benefit to the WIT contract

The WIT `view` variant currently describes layout in iyon-tui's own terms
(horizontal/vertical/grid with track sizes). With Taffy, it would describe
flexbox/grid style properties that are layout-engine-independent at the WIT
level. Both the Rust public API and the engine would use Taffy internally,
making the lowering from WIT to engine a direct style translation rather than
a custom track-to-Taffy conversion.

### Not a priority

This is not a near-term task. The current layout engine works, is well-tested,
and covers the layout patterns Iyon needs today. Taffy adoption becomes
relevant when:

- Consumers need flexbox features the custom engine doesn't support
  (percentage sizing, `align-self`, `justify-content`, wrap)
- The maintenance burden of a custom layout engine outweighs the dependency cost
- The symmetric WIT split makes a shared layout representation more valuable

When it happens, it should be a behind-the-scenes swap — the public API
(including the WIT View IR) stays the same, only the engine path changes.

---

## Future idea: symmetric WIT split

Current architecture: Rust code produces `iyon_tui::View` (a Rust struct), WASM
plugins produce WIT `view`, and `iyon-plugins/` converts WIT → Rust View before
the engine touches it. Two producer paths, one conversion step.

Proposed: split `iyon-tui` into a public API crate that lowers into WIT records
and an engine crate (`iyon-tui-core`) that consumes WIT exclusively.

```
iyon-tui (public API crate)
  - View, Text, TextSpan, Horizontal, Vertical, Grid, IntoView
  - StyleSpec, ColorSpec, Insets, BorderSpec, ThemeKey
  - Component trait, ComponentHandle, ComponentCx, Output, EventCx
  - Fluent construction API
  - ↓ lowers into WIT records ↓

iyon-tui-core (engine crate)
  - Consumes WIT records as input (never sees Rust View)
  - Layout, paint, theme resolution, surface composition
  - App framework, History, ScrollPane, StreamPane
  - Scene host, component registry, focus management, tick scheduling
```

This makes the surface symmetric:

| Producer | Produces |
|---|---|
| Rust application code | `iyon-tui` → WIT |
| WASM plugin (Rust) | `iyon-tui` compiled into WASM → WIT |
| WASM plugin (JS/TS) | `iyon-view` → WIT |
| WASM plugin (Zig/Go/C) | hand-rolled WIT |

No special host-side conversion for plugins. The engine only sees WIT. A Rust
plugin compiled to WASM uses the same `iyon-tui` crate as the host application
(both lower to the same WIT records).

**What this requires:** the current WIT `view` variant only describes `ViewKind`
(text, horizontal, vertical, grid, hanging, spacer, clamp, styled, container).
It does not carry the shell properties that `iyon_tui::View` wraps around the
kind: `width`/`height` rules, `padding`, `border`, `background`, `style_states`,
`component` slots. These would need to fold into the WIT contract as a
`view-node` record that wraps the kind variant. Missing shell fields must be
discovered and added before the split is viable.

---

## Non-goals (explicitly out of scope for now)

- **No plugin marketplace/distribution system** — just files in a directory.
- **No version negotiation** — first version is simple: load or fail.
- **No hot-reload** — plugins loaded at startup; restart to pick up changes.
- **No capability-based security** — first version: WASM sandbox is the only boundary.
- **No language bindings for Python/TS application writers** (PyO3, napi-rs) — WASM + WIT is the universal plugin path. Python/TS users write plugins in their language and compile to WASM.
- **No changes to the History, presenter, ShadowTerminal, or native scroll pipeline.**

---

## Immediate next steps (not yet)

This document is a design capture, not a task list. The streamed tool-invocation lifecycle (C1–C6) is the current priority. Plugin architecture becomes relevant when:

- The built-in tools feel heavy compiled into the binary
- A real third-party tool integration is desired
- The WIT toolchain has settled enough to commit to

Estimated: **7+ days** at current development velocity.

---

## Open questions for the planning agent

1. **WIT toolchain maturity** — `wit-bindgen` for Rust guests is mature. What about Go, Zig, C? Does the contract need to allow hand-written WAT for minimal plugins?
2. **Async WASM execution** — wasmtime supports async; do we need a dedicated async executor per plugin instance, or is a pooled thread-per-plugin sufficient?
3. **Plugin discovery** — scan `~/.iyon/plugins/*.wasm` at startup? Manifest file alongside for metadata (name, version, author)?
4. **Renderer-only plugins** — if a plugin only provides rendering (e.g., a syntax highlighter) without a tool handler, does it still need a tool entry point?
5. **Versioning the WIT contract** — how to handle evolution without breaking existing plugins?
6. **Stateful plugins** — should a plugin be able to hold state between calls (e.g., a database connection pool) or is every call stateless?
7. **Built-in migration order** — should we migrate tools first (simplest), then renderers, then providers, or all at once?
8. **Registration conflicts** — if two plugins register the same tool name, should the last one win, the first one win, or should it error?
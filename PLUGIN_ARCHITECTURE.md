# Iyon Plugin Architecture

> Captured 15 August 2026 — design discussion distilled for the planning agent.

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

---

## The chosen approach: WASM via WIT

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

---

## The WIT contract (sketch per domain)

### `iyon-api/plugins/` — model providers

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

### `iyon-core/plugins/` — tools

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

### `iyon-tui/plugins/` — rendering and UI

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

---

## Built-in components as plugins

The current compiled-in components should eventually migrate to the plugin interface:

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

Each `.wasm` defines both the tool's **execution logic** and its **renderer** in one unit. The current split (handler in `iyon-core`, renderer in `iyon/tui/tools/renderers`) is a legacy of the pre-plugin architecture and should be unified.

**Dogfood principle:** If the WIT interface is ergonomic enough for the built-in tools, providers, and renderers, it's ergonomic enough for third-party plugins. Discomfort during built-in migration is a signal to fix the contract before releasing it publicly.

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

## Non-goals (explicitly out of scope for now)

- **No plugin marketplace/distribution system** — just files in a directory.
- **No version negotiation** — first version is simple: load or fail.
- **No hot-reload** — plugins loaded at startup; restart to pick up changes.
- **No capability-based security** — first version: WASM sandbox is the only boundary.
- **No language bindings for Python/TS application writers** (PyO3, napi-rs) — WASM + WIT is the universal plugin path. Python/TS users write plugins in their language and compile to WASM.
- **No changes to the History, presenter, ShadowTerminal, or native scroll pipeline.**

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

---

## Immediate next steps (not yet)

This document is a design capture, not a task list. The streamed tool-invocation lifecycle (C1–C6) is the current priority. Plugin architecture becomes relevant when:

- The built-in tools feel heavy compiled into the binary
- A real third-party tool integration is desired
- The WIT toolchain has settled enough to commit to

Estimated: **7+ days** at current development velocity.
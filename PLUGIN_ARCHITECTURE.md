# Iyon Plugin Architecture

> Captured 15 August 2026 — design discussion distilled for the planning agent.

## The Goal

Allow third-party developers to extend Iyon with:

1. **Tool implementations** — new capabilities (e.g. `docker`, `kubectl`, `db`, `weather`, `github`)
2. **Tool renderers** — custom display for those tools (tables, colored output, progress, spinners, etc.)

All at runtime, post-compile, without recompiling Iyon, `iyon-core`, or `iyon-tui`.

---

## Guiding principles

- The plugin system must **not** compromise Iyon's streaming, History, presentation, or native scroll architecture.
- The plugin boundary should **clean the architecture**, not muddy it — tools and renderers currently compiled into `iyon` should move into plugins too.
- `iyon-tui` is a generic Rust TUI toolkit. It should **not** become WASM-aware or plugin-aware.
- `iyon-core` is a generic agent runtime. It should **not** know about plugins or tool renderers.
- The plugin system lives in **the Iyon application layer** (the binary), not in the libraries.

---

## The chosen approach: WASM via WIT

**WASM** was chosen over:

| Approach | Rejected because |
|---|---|
| **Rhai** (embedded scripting) | Plugin authors are locked into Rhai; real I/O requires registering every host function; Views from Rhai are awkward |
| **Stdio/MCP protocol** | Slower (serialization + subprocess overhead per call), harder to distribute as single file, language-agnostic but leaky |
| **Dynamic linking (`dylib`)** | Platform-specific, unsafe, Rust ABI instability |

WASM gives:

- **Language freedom** — plugins in Rust, Go, C, Zig, AssemblyScript, TinyGo, Python (via wasmtime-py), etc.
- **Sandboxing** — no arbitrary memory access, no unsafe Rust risk
- **Single file distribution** — one `.wasm` file per plugin
- **Performance** — near-native execution speed via wasmtime/wasmer
- **Deterministic interface** — WIT contract is the explicit API boundary

---

## The plugin contract (WIT sketch)

A plugin declares two capabilities: **tools** and **renderers**.

```wit
// plugin.wit

/// What the plugin provides
interface plugin {
    /// Declare what tools this plugin handles
    list-tools: func() -> list<tool-spec>

    /// Execute a tool call
    execute-tool: func(name: string, args: json) -> stream<tool-event>
}

/// What the host (Iyon) provides to the plugin
interface host {
    read-file: func(path: string) -> result<string, error>
    write-file: func(path: string, content: string) -> result<_, error>
    exec-command: func(command: string, args: list<string>) -> result<string, error>
    log: func(level: log-level, message: string)
    http-request: func(request: http-request) -> result<http-response, error>
    // ... expand as needed
}

record tool-spec {
    name: string,
    description: string,
    input-schema: json,
}

variant tool-event {
    text(string),
    progress { label: string, current: option<u64>, total: option<u64> },
    details(json),
    result { text: string, details: json, is-error: bool },
}
```

### Renderer interface (optional per plugin)

If a plugin ships renderers, it also implements:

```wit
interface renderer {
    render-call: func(input: call-state) -> render-tree
    render-result: func(input: result-state) -> render-tree
}

record call-state {
    tool-name: string,
    arguments: json,
    status: tool-status,
}

record result-state {
    tool-name: string,
    text: string,
    details: json,
    is-error: bool,
    collapsed: bool,
}

variant tool-status {
    preparing,
    prepared,
    pending-approval,
    approved,
    running,
    finished,
    failed,
    rejected,
    cancelled,
}
```

---

## The render-tree IR

Plugins return a **render-tree** — a serializable, language-neutral display description. This is *not* `iyon-tui::View` — it's a data structure that gets lowered *into* View by Iyon's application layer.

```wit
variant render-tree {
    empty,
    text(content: string, style: option<style-spec>),
    horizontal(children: list<render-tree>, gap: option<u16>),
    vertical(children: list<render-tree>, gap: option<u16>),
    table(headers: list<string>, rows: list<list<string>>),
    code-block(language: option<string>, content: string),
    spinner(label: string, active: bool),
    progress-bar(label: string, current: u64, total: u64),
    collapse(summary: render-tree, details: render-tree, expanded: bool),
    styled(child: render-tree, style: style-spec),
    hanging(indent: render-tree, continuation: render-tree, content: render-tree),
    clamp-rows(max: u16, overflow-indicator: render-tree, child: render-tree),
}

record style-spec {
    bold: option<bool>,
    dim: option<bool>,
    italic: option<bool>,
    underline: option<bool>,
    fg: option<color>,
    bg: option<color>,
}

variant color {
    ansi(u8),
    rgb(u8, u8, u8),
    theme(string),
}
```

### Lowering

A small `RenderTree → View` converter lives in **the Iyon application layer** (not in `iyon-tui`). It's a pure function that recursively maps render-tree nodes to `iyon_tui::View` constructors:

```rust
fn lower_render_tree(tree: RenderTree, theme: &Theme) -> View {
    match tree {
        RenderTree::Text { content, style } => {
            let mut v = View::text(content);
            if let Some(s) = style { v = v.with_style(lower_style(s, theme)); }
            v
        }
        RenderTree::Table { headers, rows } => {
            // Lower to iyon_tui::Grid
        }
        RenderTree::CodeBlock { language, content } => {
            // Lower using TextRenderer + MarkdownProjector
        }
        // ...
    }
}
```

This is the **only** new component needed. `iyon-tui` doesn't change. The layout/paint engine receives `View` as always.

---

## Architectural impact

### Current state

```
iyon-core (compiled-in tools: bash, read, write, grep, ...)
    + iyon-tui (no plugin awareness)
    = iyon binary with hardcoded ToolRendererRegistry
```

### Future state

```
iyon-core (pure lifecycle engine — no tools, no renderers)
    + iyon-tui (pure Rust TUI toolkit — no plugin awareness)
    + iyon-plugin-host (WASM runtime + render-tree lowerer)
    + iyon-builtin-tools/*.wasm (shipped with binary, same WIT interface)
    = iyon binary with dynamic plugin loading
```

The `ToolRendererRegistry` becomes a thin bridge that loads WASM instances and calls their `render-call` / `render-result` WIT exports.

---

## Built-in tools as WASM

The tools currently compiled into Iyon (bash, read, write, ls, grep, find, edit) should move into individual WASM modules:

```
crates/iyon-tools/
├── bash/
│   ├── Cargo.toml         # builds to bash.wasm
│   ├── src/lib.rs         # implements WIT plugin + renderer interfaces
│   └── render.rs          # optional: bash-specific render-tree construction
├── read/
├── write/
├── ls/
├── grep/
├── find/
├── edit/
└── generic/               # fallback renderer for tools without custom renderers
```

They ship in the binary's plugin directory (`~/.iyon/plugins/` or embedded) and load through the exact same path as third-party plugins. Users can:

- Delete a `.wasm` to disable a tool
- Drop in a replacement to override behavior or display
- Toggle on/off via config (registry skips unloaded plugins)

**Dogfood principle:** If the WIT interface is ergonomic enough for the built-in tools, it's ergonomic enough for third-party plugins. Discomfort during built-in migration is a signal to fix the contract before releasing it publicly.

---

## Non-goals (explicitly out of scope for now)

- **No plugin marketplace/distribution system** — just files in a directory.
- **No version negotiation** — first version is simple: load or fail.
- **No hot-reload** — plugins loaded at startup; restart to pick up changes.
- **No capability-based security** — first version: WASM sandbox is the only boundary.
- **No generic "tool call" concept in `iyon-tui`** — tool semantics remain Iyon application semantics.
- **No changes to the History, presenter, ShadowTerminal, or native scroll pipeline.**

---

## Open questions for the planning agent

1. **Render-tree serialization format** — JSON (human-readable, familiar) or MessagePack/binary (faster, smaller)?
2. **Async tool execution** — WASM plugins can stream tool events; does the host need a dedicated async executor per plugin instance, or is a pooled thread-per-plugin sufficient?
3. **Plugin discovery** — scan `~/.iyon/plugins/*.wasm` at startup? Manifest file alongside?
4. **WIT toolchain** — `wit-bindgen` for Rust guest plugins is mature. What about Go, Zig, C? Does the contract need to allow hand-written WAT for minimal plugins?
5. **Renderer vs no-renderer plugins** — if a plugin doesn't implement the renderer interface, fall back to a generic renderer that shows name + args + result text. Does that live in the host or ship as a `generic.wasm`?
6. **Versioning the WIT contract** — how to handle evolution without breaking existing plugins?
7. **Shared vs per-call instances** — does each tool call get a fresh WASM instance, or does a plugin instance persist across calls?
8. **Stateful plugins** — should a plugin be able to hold state between calls (e.g., a database connection) or is every call stateless?

---

## Immediate next steps (not yet)

This document is a design capture, not a task list. The streamed tool-invocation lifecycle (C1–C6) is the current priority. Plugin architecture becomes relevant when:

- The built-in tools feel heavy compiled into the binary
- A real third-party tool integration is desired
- The WIT toolchain has settled enough to commit to

Estimated: **7+ days** at current development velocity.
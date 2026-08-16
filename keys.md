# FFI is a Future thing (not now), this is relevant now anyway

I checked the actual code path. The answer is: **yes — in the current architecture, ordinary keypresses should not go through Bun at all.** I was incorrectly flattening your runtime into a JS-style event loop.

The real path in `iyon-tui` is:

```text
crossterm Event
    ↓
terminal::crossterm::map_event()
    ↓
private TerminalEvent::Key(KeyStroke)
    ↓
RunningApp::dispatch_key()
    ↓
SceneHost::dispatch_key_local()
    ↓
route_key_local()
    ↓
focused component → ancestors
    ↓
ComponentCx::key_commands()
    ↓
component mutates itself
    ↓
EventCx::emit(Output<T>, value)
    ↓
OutputRouter
    ↓
application Action
```

And almost **that whole chain is already inside Rust**.

The terminal backend itself is private. `TerminalEvent` is a private enum containing `Key`, `Paste`, and `Resize`, and `TerminalBackend::next_event()` never exposes that event stream as a public app API.  Crossterm is read on its own blocking `iyon-terminal-input` thread, normalized into that private `TerminalEvent`, and fed into the runtime. 

Then the application driver consumes `TerminalEvent::Key` by calling `app.dispatch_key(key)`. It does not call application code with the key. 

## The important bit: `dispatch_key`

This is the actual implementation:

```text
RunningApp::dispatch_key
    │
    ├── SceneHost::dispatch_key_local
    │        ↓
    │   focused component routing
    │
    ├── drain component outputs → actions
    │
    └── only if Ignored:
             global binding → Action
```

`RunningApp::dispatch_key()` first invokes `scene_host.dispatch_key_local()`, immediately drains component outputs into actions, and only if the local route returned `Ignored` does it consult `GlobalBindings<Action>`. If a local component consumed the key, it merely marks the frame dirty. 

So yes: **the application does not receive every key.**

And `GlobalBindings` itself is already exactly what you remembered:

```rust
HashMap<KeyStroke, Box<dyn Fn() -> Action>>
```

The framework maps the key directly to an application action. 

---

The component side is even stronger.

`ComponentCx::key_commands()` installs two native functions:

```text
KeyStroke
   ↓
map(&Component, KeyStroke)
   ↓
typed Command
   ↓
handle(&mut Component, Command, EventCx)
   ↓
InteractionResult
```

Those capabilities are retained by the mounted component system. 

`route_key_local()` then takes the host-owned focus state, constructs the routing chain from the focused component upward through its ancestors, invokes each component's native key-command mapper/handler, and stops when something consumes the event. **Tab focus traversal itself also happens right there in Rust** after component routing. 

So:

```text
keypress
   ↓
focused TextInput
   ↓
TextInputCommand
   ↓
TextInput mutation
```

doesn't need the application at all.

---

## `TextInput` proves it

This isn't theoretical.

`TextInput::capabilities()` currently does:

```rust
cx.focusable();
cx.on_focus_changed(Self::focus_changed_callback);
cx.key_commands(Self::command_for_key, Self::handle_command);
cx.on_paste(Self::paste_callback);
cx.on_layout_changed(Self::layout_changed);
```



So when you type `a`:

```text
terminal "a"
   ↓
Rust KeyStroke
   ↓
native focused-component routing
   ↓
TextInput::command_for_key()
   ↓
TextInputCommand::Insert(...)
   ↓
TextInput::handle_command()
   ↓
native TextBuffer mutation
   ↓
component becomes visually different
```

**Bun has absolutely nothing useful to do in that path.**

Cursor-left, cursor-right, backspace, word movement, multiline editing, etc. are all precisely the kind of local interaction that should stay completely native.

---

## Outputs are the boundary

This is the part I had wrong before.

Components don't tell the application:

> User pressed Enter.

They tell the application:

> I emitted my semantic `submitted` output.

`TextInput` owns a stable:

```rust
submitted: Output<String>
```

and exposes:

```rust
pub fn submitted(&self) -> Output<String>
```

It can also create projected typed change outputs. 

`EventCx::emit()` puts a typed output into the native `OutputQueue`. 

Then `OutputRouter<Action>` maps:

```text
Output<T>
    ↓
Fn(T) -> Action
    ↓
Action
```

and `RunningApp` puts those actions into its action queue.  

**That** is the seam we need to project across FFI for a TS-owned application.

Not keys.

---

# So the proper FFI architecture is different from my previous handoff

The ordinary runtime path should be:

```text
                         RUST / iyon-tui
────────────────────────────────────────────────────────

terminal
   ↓
TerminalEvent
   ↓
native input router
   ↓
FocusState
   ↓
focused component routing
   ↓
native component commands
   ↓
native component state mutation
   ↓
Output<T>
   ↓
native output routing
   ↓
RoutedAction

──────────────────── FFI boundary ───────────────────────
                         ↓
                    ActionId
                    payload
                         ↓
────────────────────────────────────────────────────────
                         BUN / TS

                    application update
                         ↓
                      state
                         ↓
                       Scene
                         ↓
                  FFI View program
```

For a key handled entirely by `TextInput`:

```text
keypress
  ↓
Rust
  ↓
TextInput mutation
  ↓
render

FFI crossings: ZERO
```

For Enter causing submission:

```text
Enter
  ↓
Rust TextInput
  ↓
Output<String>("hello")
  ↓
route
  ↓
ActionId::Submit + "hello"
  ↓
FFI
  ↓
TS update()
```

One semantic crossing.

That's the right boundary.

---

# Focus is also native already

`FocusState` is explicitly documented in source as **host-owned semantic focus state**. It tracks the focused component, modal scope, modal restoration stack, geometry, and focus-change handler. 

During scene reconciliation, `SceneHost` recalculates mounted capabilities and geometry, then calls `focus.reconcile_with_geometry(...)`. Focus changes can invoke the component's native `on_focus_changed` callback, and the final focused component is also passed into `ViewCompiler` for interaction-aware styling. 

So again:

```text
TS manually tracking focus
```

would be a regression.

The native framework already owns:

```text
mount tree
focus eligibility
visibility/geometry eligibility
modal scopes
focus restoration
Tab traversal
focus callbacks
focused styling state
```

That all stays Rust.

---

# Paste follows essentially the same model

`dispatch_paste()` first offers a focused/modal-chain **application interceptor**, which can produce an action. Otherwise it routes the paste directly through the component system and drains emitted outputs. 

So even paste doesn't inherently need to become a Bun event.

For a normal focused `TextInput`:

```text
terminal paste
    ↓
native TextInput paste handler
    ↓
native text mutation
```

Only semantic outputs requested by application policy need to escape.

---

# What changes because the app itself becomes TypeScript?

This is the one real adaptation we need.

Today Rust can store:

```rust
Fn(T) -> Action
Fn() -> Action
FnMut(&mut State, Action, &mut AppCx)
```

because the app itself is Rust.   

We obviously can't put a TS closure into the future pure C-FFI runtime without callbacks.

So the FFI projection should replace those **application-side closure mappings**, not the native input machinery.

Conceptually:

```text
Current Rust

KeyStroke
  → Fn() -> Action

Output<T>
  → Fn(T) -> Action


FFI / TS app

KeyStroke
  → RoutedActionId

Output<T>
  → RoutedActionId + generated payload projection
```

For example:

```ts
app.bindKey(Key.ctrl("c"), Actions.cancel);
app.route(input.submitted(), Actions.submit);
```

could compile/configure native tables like:

```text
Ctrl+C
  → action_id 12

TextInput.submitted
  → action_id 37
  → payload codec String
```

Then Rust enqueues:

```text
{ action_id: 37, payload: "hello" }
```

and **that** crosses the FFI boundary.

This preserves the architecture almost exactly.

---

# Native component vs TS component

There's one useful distinction the handoff should explicitly make.

A **native Rust component** such as `TextInput` keeps everything local:

```text
KeyStroke
  → Rust command map
  → Rust command handler
  → mutation/output
```

A **custom TS component** cannot install an arbitrary Rust function pointer like today's:

```rust
fn(&C, KeyStroke) -> Option<Command>
```

without introducing callbacks.

So its TS façade should normally register declarative bindings:

```ts
component.bind(Key.enter, Actions.activate);
component.bind(Key.escape, Actions.cancel);
```

Native routing still matches the key, but the resulting semantic action crosses to TS.

If someone genuinely needs arbitrary state-dependent key inspection in a TS component, we can have an opt-in capability whose native route emits:

```text
TsComponentKey {
  component_id,
  KeyStroke
}
```

But that's an **escape hatch for that component**, not the runtime's default input architecture.

So even then we do not:

```text
every keyboard event → Bun
```

We do:

```text
only keys intentionally delegated to TS → Bun
```

---

# Comparing that with other TUIs makes the difference pretty stark

OpenTUI currently explicitly exposes `renderer.keyInput` as a TypeScript `EventEmitter` producing structured `keypress` events. Its docs also say focused components receive keyboard input. 

That means its normal architecture really does move keyboard input into JS/TS.

There are consequences to that model: OpenCode has a current issue where a focused OpenTUI textarea consumes Enter itself, so the higher-level declarative keymap never sees the event. 

Pi's TUI does essentially the same thing at an even lower level: its `Component` interface has optional `handleInput(data: string)`, and the docs state that the focused component receives the **raw terminal input string**, including ANSI escape sequences. Focus is managed by the TypeScript TUI itself. 

Iyon is already architecturally different:

```text
OpenTUI
terminal → parsed key → TS focused component

Pi
terminal → raw terminal input → TS focused component

Iyon
terminal → semantic KeyStroke → Rust focused component
                                 ↓
                              Output
                                 ↓
                        application Action
```

For the system you're building, **I would preserve Iyon's model rather than imitate either of those.**

---

## The concrete correction to the FFI plan

The sections I previously wrote around:

```text
native event queue
→ drain KeyStroke events
→ TS event pump
```

should be removed.

The replacement should be:

```text
NATIVE INTERACTION HOST

terminal input
focus
modal routing
component key commands
paste handlers
ticks
layout callbacks
native component mutation
OutputQueue

        ↓

APPLICATION ROUTE TABLE

global KeyStroke → ActionId
Output<T>        → ActionId + generated payload codec
timer            → ActionId

        ↓

FFI ACTION QUEUE

ActionId + payload

        ↓

TS application
```

`Key` and `KeyStroke` still exist in the generated TS API because TS needs them to **declare bindings**, configure components, build keymaps, and test input. But they are not ordinary runtime events crossing FFI.

That matches the actual framework you already wrote.   

And this changes my opinion of the FFI architecture in a good way: `iyon-tui` can remain far more autonomous than I was giving it credit for. The Rust side isn't merely a renderer behind Bun; it is a retained **UI runtime** with native interaction, focus, component state, projection, layout, history, and rendering. Bun is the application/product layer that consumes semantic outputs/actions and supplies semantic state/scene changes.

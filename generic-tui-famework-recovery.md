 The old-app audit confirms the distinction. I inspected origin/bun-refactor’s
 pre-merge crates/iyon and traced its iyon_tui usage.

 The failed pending_style edit was reverted; no code changes remain.

 What is correctly generic

 These are legitimate iyon-tui concepts and should remain native/generic:

 - View, Component, ComponentHandle
 - Scene, App, AppCx
 - History, HistoryLayout, FlowBoundary
 - ScrollPane, TextInput, ViewSlot
 - TextStream, StreamingSource, Projection, Smooth
 - MarkdownProjector, TextRenderer
 - DiffHunk, DiffRenderer
 - Theme, styles, semantic tags
 - terminal input, layout, painting, scrollback, headless rendering

 The old Rust app used them exactly as generic primitives. For example:

 - crates/iyon/src/tui/mod.rs:29-42
 - crates/iyon/src/tui/state.rs:123-147
 - crates/iyon/src/tui/state.rs:244-340
 - crates/iyon/src/tui/transcript/pipeline.rs:1-12

 Those usages are the model for what the TypeScript app should do through bindings.

 Actual violations

 ### 1. Assistant semantics were moved into iyon-tui

 crates/iyon-tui/src/application/host.rs contains:

 - HostStreamSegmentKind::Thinking — lines 607-610
 - HostAssistantSegmentKind — lines 613-617
 - HostAssistantPipeline — lines 625-666
 - ThinkingMap / ThinkingRewriter — lines 685-822
 - hard-coded SemanticTag::new("app", "thinking") — line 722
 - assistant-specific stream state — lines 1064-1075
 - assistant-specific markdown/smoothing pipeline — lines 1136-1144
 - thinking-to-text newline insertion — lines 1161-1175
 - "text" / "thinking" stream snapshots — lines 1335-1351

 This is a direct relocation of:

 ```text
   origin/bun-refactor:
   crates/iyon/src/tui/transcript/pipeline.rs
   crates/iyon/src/tui/transcript/assistant_stream/mod.rs
   crates/iyon/src/tui/transcript/semantic.rs
 ```

 Those files explicitly belonged to the Iyon application. They should now live in
 plugins/app/iyon, not iyon-tui.

 ### 2. The generic stream binding is actually an assistant stream

 Rust already has a generic iyon_tui::stream::TextStream with push, snapshot, seal,
 and compaction.

 Instead, the native bridge binds HostTextStream:

 - crates/iyon-native/src/tui.rs:785-850
 - crates/iyon-tui/src/application/host.rs:1115-1590

 The public TS API exposes:

 ```ts
   appendSegment(kind: "text" | "thinking", text: string)
 ```

 at:

 - packages/iyon-runtime/src/tui/stream.ts:4-10
 - packages/iyon-runtime/src/tui/types.ts:53-73

 That makes a generic TUI stream impossible to use for logs, compiler output, file
 tails, or arbitrary semantic streams without pretending everything is assistant
 text/thinking.

 The proper generic API should be closer to:

 ```ts
   stream.append(text)
 ```

 with optional generic semantic metadata supplied by the application.

 ### 3. HostWorking is a port of ConversationActivity

 crates/iyon-tui/src/application/host.rs:137-604 contains:

 - active_label
 - pending_label
 - queue_prefix
 - pending: Vec<String>
 - queue preview rendering
 - working/waiting direction semantics
 - set_active
 - set_pending

 This is not merely a generic spinner. It is the old application’s:

 ```text
   crates/iyon/src/tui/components/conversation_activity.rs
   crates/iyon/src/tui/components/steering_queue.rs
 ```

 The queue display and “Working”/“waiting” choreography belong to the Iyon app plugin.

 The generic layer may provide a reusable animation/component primitive, but it must
 not know about queued steering messages or working-state labels.

 ### 4. The native bridge exports product-shaped APIs

 These expose the above violations directly:

 - NativeWorking — crates/iyon-native/src/tui.rs:265-298
 - NativeTuiHost::working() — lines 574-585
 - NativeTextStream.appendSegment() — lines 819-834
 - activity_config() with activeLabel, pendingLabel, queuePrefix — lines 1530-1590

 The TS façade repeats the same product concepts:

 - WorkingActivity — packages/iyon-runtime/src/tui/working.ts
 - WorkingActivityOptions — packages/iyon-runtime/src/tui/types.ts:197-205
 - Tui.createWorking() defaults to "Working", "waiting", and "Queue: " —
   runtime.ts:88-99

 The app plugin is correctly where the Iyon behavior is being orchestrated, but it is
 forced through an application-specific runtime API.

 ### 5. Rust owns an application runtime instead of exposing the generic TUI

 TuiHost creates and owns a fixed Rust application:

 - HostState, HostAction, and HostRunning — host.rs:58-95
 - TuiApp::new(...) — host.rs:1897-1906
 - internal component registration/routing
 - internal action queue
 - internal update/render loop

 Most importantly:

 ```rust
   pub async fn wait_for_action(...)
 ```

 at host.rs:2251-2268 explicitly keeps terminal input, ticks, stream wakeups, and
 rendering on the Rust side.

 That contradicts the Bun handoff’s required ownership:

 ```text
   TS owns control flow and application state
   Rust owns terminal/layout/history/rendering mechanics
 ```

 The current bridge returns only:

 ```text
   { action_id, payload }
 ```

 rather than exposing raw generic terminal events and allowing TS to run the
 application loop.

 ### 6. Rust owns application-level action routing

 These are application-loop APIs, not a thin generic TUI membrane:

 - bind_key(key, action_id) — host.rs:1972-1981
 - route_text_input(..., action_id) — host.rs:2039-2073
 - intercept_paste(..., action_id) — host.rs:2076-2095
 - nextAction() / waitForAction() — tui.rs:680-701

 The TS runtime mirrors this:

 - runtime.ts:53-65
 - runtime.ts:113-125
 - types.ts:212-229

 nextEvent() is currently only a compatibility shim that returns a synthetic terminate
 event; it does not expose key, paste, or resize events.

 The old Rust app’s typed App<Action> routing should become TS application logic. The
 native layer should expose generic events/outputs, not string action IDs.

 ### 7. Generic output/component binding is incomplete

 The native bridge only supports:

 ```rust
   NativeTuiOutput { output: Output<String> }
 ```

 and routes it through string action IDs.

 That cannot represent the generic Rust API’s arbitrary typed outputs. The old app
 used outputs such as:

 ```text
   Output<ApprovalDecision>
   Output<String>
 ```

 Also, ComponentAdapterBridge exists only as a local TS promise wrapper:

 ```text
   packages/iyon-runtime/src/tui/traits/component.ts
 ```

 It is never connected to the native component registry. The TS Component class is
 effectively a NativeViewSlot placeholder, not a native mountable TS-defined
 component.

 Therefore the claimed full generic Component/trait binding is not actually present.

 ### 8. App-specific defaults are hardcoded in the native host

 These were copied from the old Iyon app:

 - History layout (padding bottom = 1, gap = 1) — host.rs:1903-1906
 - TextInput top/bottom border — host.rs:1933-1937
 - Markdown stream insets (0, 2, 0, 2) — iyon-native/src/tui.rs:795-798

 The old app explicitly chose these in:

 ```text
   crates/iyon/src/tui/mod.rs
   crates/iyon/src/tui/state.rs
   crates/iyon/src/tui/theme.rs
 ```

 They should be supplied by the TS app/plugin or generic configuration, not selected
 by the generic native host.

 What is not a violation

 The following should not be removed merely because Iyon uses them:

 - History-based scrollback
 - Markdown projection
 - Diff projection
 - generic smoothing/projection
 - ScrollPane
 - TextInput
 - native layout and painting
 - native terminal scrollback
 - generic semantic annotations

 The old app’s use of those APIs proves they are reusable framework capabilities. The
 violation is that the current bridge moved Iyon’s policy and data model into the
 framework instead of binding those generic capabilities faithfully.

 The main architectural correction is:

 ```text
   plugins/app/iyon
     owns assistant/thinking/steering/working/tool policy,
     stream composition, theme, event reduction, and app loop

   iyon-runtime / iyon-native
     exposes generic TUI values, handles, events, components,
     History, TextStream, Projection, Markdown, Diff, and rendering

   iyon-tui
     remains terminal/layout/history/rendering infrastructure only
 ```

## Recovery implementation

The boundary violations above have been removed from the generic TUI path:

- `HostWorking`, `HostActivityConfig`, steering queue fields, working/waiting labels, and product spinner policy were removed from `iyon-tui`, `iyon-native`, and the runtime facade. `plugins/app/iyon` now composes the existing generic `ViewSlot` API with caller-supplied `View[]` frames and an 80ms interval. Rust owns slot identity, timers, frame selection, invalidation, and rendering; TypeScript updates the frame set only when application state changes. Native key handling remains in `iyon-tui`.
- `HostAssistantPipeline`, `HostStreamSegmentKind::Thinking`, `ThinkingMap`, and `ThinkingRewriter` were replaced by a generic Markdown text pipeline with caller-supplied namespaced `TextStreamAnnotation` values. The Iyon plugin supplies `app:thinking` and performs thinking-to-text separator normalization itself.
- `TextStream` now exposes generic `append(text, annotations?)`, generic snapshots, Markdown presentation options, and caller-controlled insets. The assistant stream is an application wrapper over that API.
- Native routed outputs are exposed to the TS facade as generic `routeId` outputs through `nextEvent`; keyboard interpretation and component interaction stay native and keystrokes do not cross N-API for application reduction. Old action names remain only as compatibility aliases for the existing headless harness.
- App-specific History layout and TextInput border defaults were removed from the native host. The Iyon plugin supplies its existing values, preserving the shipped UI.
- Generic host mutations invalidate and render immediately, preserving stable native handles while making History, streams, slots, scroll panes, and inputs usable through the TS facade.

The public generic animation surface is intentionally the existing `ViewSlot.setAnimation(frames, intervalMs)` contract rather than a working-spinner API. This keeps frame ticking native for the performance architecture while allowing any caller to supply arbitrary views and timing.

//! A language-binding host for the retained native application runtime.
//!
//! `TuiHost` deliberately exposes semantic actions and native snapshots, not
//! terminal events. Components remain mounted in the same `SceneHost` used by
//! the Rust application driver.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, Weak},
    time::{Duration, Instant},
};

use anyhow::Result;

use crate::{
    AppCx, App as TuiApp, BorderEdges, BorderSpec, Component, ComponentCx,
    ComponentHandle, History, HistoryLayout, InteractionResult, KeyStroke, Output, TextInput,
    HistoryStreamHandle, IntoView, StreamOffset, StreamRange, StreamRevision, StreamSnapshot,
    StreamSnapshotBuilder, StreamingSource, StyleRef, TextContent, Theme, View,
    MarkdownOptions, MarkdownProjector, ProjectionBuilder, Projector, Smooth,
    backend::NativeHistorySink,
    geometry::Size,
    physical::PhysicalRow,
    scene::PreparedSceneFrame,
    terminal::{TerminalBackend, TerminalEvent, termwiz::TermwizBackend},
};
use crate::controls::text_input::command::TextInputCommand;
use crate::text::{TextRun, TextVisitor};

/// One application-level action produced by native interaction routing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoutedAction {
    pub action_id: String,
    pub payload: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostCellStyle {
    pub foreground: Option<String>,
    pub background: Option<String>,
    pub bold: bool,
    pub dim: bool,
    pub italic: bool,
    pub underline: bool,
    pub reversed: bool,
    pub strikethrough: bool,
}

#[derive(Debug)]
enum HostAction {
    Routed(RoutedAction),
}

struct HostState {
    body: View,
    actions: VecDeque<RoutedAction>,
}

fn host_init(_cx: &mut AppCx<'_, HostAction>) -> Result<HostState> {
    Ok(HostState {
        body: View::spacer(0),
        actions: VecDeque::new(),
    })
}

fn host_update(
    state: &mut HostState,
    action: HostAction,
    _cx: &mut AppCx<'_, HostAction>,
) -> Result<()> {
    match action {
        HostAction::Routed(action) => state.actions.push_back(action),
    }
    Ok(())
}

fn host_view(state: &HostState) -> View {
    state.body.clone()
}

type HostRunning = crate::application::kernel::RunningApp<
    HostState,
    HostAction,
    anyhow::Error,
    fn(&mut HostState, HostAction, &mut AppCx<'_, HostAction>) -> Result<()>,
    fn(&HostState) -> View,
>;

#[derive(Default)]
struct HeadlessSink {
    width: u16,
    height: u16,
    history: Vec<PhysicalRow>,
}

impl NativeHistorySink for HeadlessSink {
    type Error = anyhow::Error;

    fn insert_history_rows(&mut self, rows: &[PhysicalRow]) -> Result<usize, Self::Error> {
        self.history.extend(rows.iter().cloned());
        Ok(rows.len())
    }
}

enum HostBackend {
    Headless(HeadlessSink),
    Real(TermwizBackend),
}

struct HostInner {
    running: HostRunning,
    backend: HostBackend,
    frame: PreparedSceneFrame,
    now: Instant,
    headless: bool,
    closed: bool,
}

/// A shared native TextInput value that can be mounted into one TuiHost.
#[derive(Clone)]
pub struct HostTextInput {
    state: Arc<Mutex<TextInput>>,
    component_id: Arc<Mutex<Option<u64>>>,
    host: Arc<Mutex<Option<Weak<Mutex<HostInner>>>>>,
}

#[derive(Clone)]
pub struct HostActivityConfig {
    pub frames: Vec<String>,
    pub active_label: String,
    pub pending_label: String,
    pub queue_prefix: String,
    pub tick_ms: u64,
    pub muted_style: StyleRef,
    pub padding: u16,
}

impl Default for HostActivityConfig {
    fn default() -> Self {
        Self {
            frames: vec!["•".to_owned()],
            active_label: "active".to_owned(),
            pending_label: "pending".to_owned(),
            queue_prefix: "queue: ".to_owned(),
            tick_ms: 80,
            muted_style: StyleRef::default(),
            padding: 0,
        }
    }
}

#[derive(Default)]
struct WorkingState {
    active: bool,
    frame: usize,
    pending: Vec<String>,
}

/// A native ticking working/status component configured by the application.
#[derive(Clone)]
pub struct HostWorking {
    state: Arc<Mutex<WorkingState>>,
    component_id: Arc<Mutex<Option<u64>>>,
    config: HostActivityConfig,
}

#[derive(Clone)]
pub struct HostViewSlot {
    state: Arc<Mutex<ViewSlotState>>,
    component_id: Arc<Mutex<Option<u64>>>,
    host: Arc<Mutex<Option<Weak<Mutex<HostInner>>>>>,
}

struct ViewSlotState {
    view: View,
    revision: u64,
}

impl HostViewSlot {
    pub fn new(view: View) -> Self {
        Self {
            state: Arc::new(Mutex::new(ViewSlotState { view, revision: 0 })),
            component_id: Arc::new(Mutex::new(None)),
            host: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_view(&self, view: View) -> Result<()> {
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("view slot lock is poisoned"))?;
            state.view = view;
            state.revision = state.revision.saturating_add(1);
        }
        self.render_host()
    }

    pub fn component_id(&self) -> Option<u64> {
        self.component_id.lock().ok().and_then(|id| *id)
    }

    pub fn revision(&self) -> u64 {
        self.state.lock().map(|state| state.revision).unwrap_or(0)
    }

    fn attach_host(&self, host: &Arc<Mutex<HostInner>>) -> Result<()> {
        *self
            .host
            .lock()
            .map_err(|_| anyhow::anyhow!("view slot host lock is poisoned"))? =
            Some(Arc::downgrade(host));
        Ok(())
    }

    fn set_component_id(&self, id: u64) -> Result<()> {
        *self
            .component_id
            .lock()
            .map_err(|_| anyhow::anyhow!("view slot component lock is poisoned"))? = Some(id);
        Ok(())
    }

    fn render_host(&self) -> Result<()> {
        let host = self
            .host
            .lock()
            .map_err(|_| anyhow::anyhow!("view slot host lock is poisoned"))?
            .clone()
            .and_then(|host| host.upgrade());
        if let Some(host) = host {
            let mut inner = host
                .lock()
                .map_err(|_| anyhow::anyhow!("host lock is poisoned"))?;
            inner.running.invalidate_frame();
            inner.render()?;
        }
        Ok(())
    }
}

struct MountedViewSlot(HostViewSlot);

impl Component for MountedViewSlot {
    fn view(&self) -> View {
        self.0
            .state
            .lock()
            .map(|state| state.view.clone())
            .unwrap_or_else(|_| View::spacer(0))
    }
}

impl HostWorking {
    pub fn new(config: HostActivityConfig) -> Self {
        Self {
            state: Arc::new(Mutex::new(WorkingState::default())),
            component_id: Arc::new(Mutex::new(None)),
            config,
        }
    }

    pub fn set_active(&self, active: bool) -> Result<()> {
        self.state
            .lock()
            .map_err(|_| anyhow::anyhow!("working component lock is poisoned"))?
            .active = active;
        Ok(())
    }

    pub fn set_pending(&self, pending: Vec<String>) -> Result<()> {
        self.state
            .lock()
            .map_err(|_| anyhow::anyhow!("working component lock is poisoned"))?
            .pending = pending;
        Ok(())
    }

    pub fn component_id(&self) -> Option<u64> {
        self.component_id.lock().ok().and_then(|id| *id)
    }

    fn set_component_id(&self, id: u64) -> Result<()> {
        *self
            .component_id
            .lock()
            .map_err(|_| anyhow::anyhow!("working component id lock is poisoned"))? = Some(id);
        Ok(())
    }
}

struct MountedWorking(HostWorking);

impl Component for MountedWorking {
    fn view(&self) -> View {
        let Ok(state) = self.0.state.lock() else {
            return View::spacer(0);
        };
        if !state.active {
            return View::spacer(0);
        }
        let frame = self.0.config.frames.get(state.frame % self.0.config.frames.len()).map(String::as_str).unwrap_or("");
        let label = if state.pending.is_empty() { &self.0.config.active_label } else { &self.0.config.pending_label };
        let status = View::text(format!("{frame} {label}")).no_wrap();
        let row = if let Some(first) = state.pending.first() {
            let preview = first.split_whitespace().collect::<Vec<_>>().join(" ");
            let extra = state.pending.len().saturating_sub(1);
            View::horizontal(|row| {
                row.gap(4);
                row.child(status);
                row.flex(
                        View::text(format!("{}{preview}", self.0.config.queue_prefix))
                        .no_wrap()
                        .style(self.0.config.muted_style.clone()),
                );
                if extra > 0 {
                    row.child(
                        View::text(format!(" + {extra} more"))
                            .no_wrap()
                            .style(self.0.config.muted_style.clone()),
                    );
                }
            })
        } else {
            status.into_view()
        };
        row.fill_width().padding(crate::Insets::horizontal(self.0.config.padding))
    }

    fn capabilities(&self, cx: &mut ComponentCx<'_, Self>) {
        cx.tick(Duration::from_millis(self.0.config.tick_ms), Self::tick);
    }
}

impl MountedWorking {
    fn tick(component: &mut Self, _now: Instant, _cx: &mut crate::EventCx<'_>) -> bool {
        let Ok(mut state) = component.0.state.lock() else {
            return false;
        };
        if !state.active {
            return false;
        }
        state.frame = state.frame.wrapping_add(1);
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostStreamSegmentKind {
    Text,
    Thinking,
}

#[derive(Clone, Debug)]
struct HostStreamSegment {
    kind: HostStreamSegmentKind,
    text: String,
}

#[derive(Default)]
struct HostStreamState {
    segments: Vec<HostStreamSegment>,
    display_text: String,
    source_base: StreamOffset,
    revision: StreamRevision,
    sealed: bool,
    markdown: Option<MarkdownProjector>,
    smooth: Option<Smooth>,
}

/// A mutable native stream shared by a History unit and its language binding.
#[derive(Clone)]
pub struct HostTextStream {
    state: Arc<Mutex<HostStreamState>>,
    host: Arc<Mutex<Option<Weak<Mutex<HostInner>>>>>,
    handle: Arc<Mutex<Option<HistoryStreamHandle<HostStreamSource>>>>,
}

impl HostTextStream {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(HostStreamState::default())),
            host: Arc::new(Mutex::new(None)),
            handle: Arc::new(Mutex::new(None)),
        }
    }

    pub fn with_markdown() -> Self {
        let stream = Self::new();
        if let Ok(mut state) = stream.state.lock() {
            state.markdown = Some(MarkdownProjector::new(MarkdownOptions::commonmark()));
            state.smooth = Some(Smooth::default());
        }
        stream
    }

    pub fn append_segment(&self, kind: HostStreamSegmentKind, text: impl AsRef<str>) -> Result<()> {
        let text = text.as_ref();
        if text.is_empty() {
            return Ok(());
        }
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("stream lock is poisoned"))?;
            if state.sealed {
                return Err(anyhow::anyhow!("stream is already sealed"));
            }
            state.segments.push(HostStreamSegment { kind, text: text.to_owned() });
            state.revision = state.revision.next();
            state.refresh_display()?;
        }
        self.render_host()
    }

    pub fn update(&self, text: impl Into<String>) -> Result<()> {
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("stream lock is poisoned"))?;
            if state.sealed {
                return Err(anyhow::anyhow!("stream is already sealed"));
            }
            state.segments = vec![HostStreamSegment {
                kind: HostStreamSegmentKind::Text,
                text: text.into(),
            }];
            state.revision = state.revision.next();
            state.refresh_display()?;
        }
        self.render_host()
    }

    pub fn seal(&self) -> Result<()> {
        {
            let mut state = self
                .state
                .lock()
                .map_err(|_| anyhow::anyhow!("stream lock is poisoned"))?;
            if state.sealed {
                return Err(anyhow::anyhow!("stream is already sealed"));
            }
            state.sealed = true;
            state.revision = state.revision.next();
            state.refresh_display()?;
        }
        self.render_host()
    }

    pub fn snapshot_json(&self) -> Result<(String, u64, bool, Vec<(String, String)>)> {
        let state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("stream lock is poisoned"))?;
        let segments = state.markdown.as_ref().map(|_| {
            state
                .segments
                .iter()
                .map(|segment| {
                    (
                        match segment.kind {
                            HostStreamSegmentKind::Text => "text".to_owned(),
                            HostStreamSegmentKind::Thinking => "thinking".to_owned(),
                        },
                        segment.text.clone(),
                    )
                })
                .collect()
        }).unwrap_or_default();
        Ok((state.display_text.clone(), state.revision.as_u64(), state.sealed, segments))
    }

    pub fn attach(&self, history: &mut History) -> Result<()> {
        let handle = history
            .push_stream(HostStreamSource { state: self.state.clone() })
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        *self
            .handle
            .lock()
            .map_err(|_| anyhow::anyhow!("stream handle lock is poisoned"))? = Some(handle);
        Ok(())
    }

    pub fn seal_history(&self, history: &mut History) -> Result<()> {
        let handle = self
            .handle
            .lock()
            .map_err(|_| anyhow::anyhow!("stream handle lock is poisoned"))?
            .as_ref()
            .copied()
            .ok_or_else(|| anyhow::anyhow!("stream is not attached to History"))?;
        history
            .seal_stream(handle)
            .map_err(|error| anyhow::anyhow!(error.to_string()))
    }

    fn attach_host(&self, host: &Arc<Mutex<HostInner>>) -> Result<()> {
        *self
            .host
            .lock()
            .map_err(|_| anyhow::anyhow!("stream host lock is poisoned"))? =
            Some(Arc::downgrade(host));
        Ok(())
    }

    fn render_host(&self) -> Result<()> {
        let host = self
            .host
            .lock()
            .map_err(|_| anyhow::anyhow!("stream host lock is poisoned"))?
            .clone()
            .and_then(|host| host.upgrade());
        if let Some(host) = host {
            let mut inner = host
                .lock()
                .map_err(|_| anyhow::anyhow!("host lock is poisoned"))?;
            if let Some(handle) = self
                .handle
                .lock()
                .map_err(|_| anyhow::anyhow!("stream handle lock is poisoned"))?
                .as_ref()
                .copied()
            {
                inner
                    .running
                    .scene_history_mut()
                    .ok_or_else(|| anyhow::anyhow!("host history is unavailable"))?
                    .refresh_stream(handle)
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            }
            inner.running.invalidate_frame();
            inner.render()?;
        }
        Ok(())
    }
}

impl HostStreamState {
    fn source_text(&self) -> String {
        self.segments.iter().map(|segment| segment.text.as_str()).collect()
    }

    fn refresh_display(&mut self) -> Result<()> {
        let source = self.source_text();
        let Some(markdown) = &mut self.markdown else {
            self.display_text = source;
            return Ok(());
        };
        let end = StreamOffset::new(source.len() as u64);
        let mut input = ProjectionBuilder::new(
            StreamOffset::ZERO,
            if self.sealed { end } else { end },
            end,
            self.sealed,
        );
        let mut cursor = StreamOffset::ZERO;
        for segment in &self.segments {
            if self.segments.len() == 1 {
                let next = cursor.saturating_add(segment.text.len() as u64);
                input = input.emit(
                    StreamRange::new(cursor, next),
                    TextContent::raw(segment.text.clone()),
                );
                cursor = next;
                continue;
            }
            for character in segment.text.chars() {
                let next = cursor.saturating_add(character.len_utf8() as u64);
                input = input.emit(StreamRange::new(cursor, next), TextContent::raw(character.to_string()));
                cursor = next;
            }
        }
        let input = input.finish()
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let input = if let Some(smooth) = &mut self.smooth {
            smooth
                .project(&input)
                .map_err(|error| anyhow::anyhow!(error.to_string()))?
        } else {
            input
        };
        let projection = markdown
            .project(&input)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        let mut display = String::new();
        for span in projection.spans() {
            for value in span.values() {
                let mut visitor = HostPlainTextVisitor { output: String::new() };
                visitor.visit_content(value);
                display.push_str(&visitor.output);
            }
        }
        self.display_text = display;
        Ok(())
    }

    fn advance(&mut self, now: Instant) -> bool {
        let Some(smooth) = &mut self.smooth else {
            return false;
        };
        if !smooth.advance(now) {
            return false;
        }
        self.refresh_display().is_ok()
    }

    fn next_wakeup(&self) -> Option<Instant> {
        self.smooth.as_ref().and_then(Smooth::next_wakeup)
    }
}

struct HostPlainTextVisitor {
    output: String,
}

impl TextVisitor for HostPlainTextVisitor {
    fn visit_raw(&mut self, raw: &crate::RawText) {
        self.output.push_str(raw.text());
    }

    fn visit_text_run(&mut self, run: &TextRun) {
        self.output.push_str(run.text());
    }
}

#[derive(Clone)]
struct HostStreamSource {
    state: Arc<Mutex<HostStreamState>>,
}

impl StreamingSource for HostStreamSource {
    fn snapshot(&self) -> StreamSnapshot {
        let state = self.state.lock().expect("host stream lock is poisoned");
        let source_text = state.source_text();
        let source_end = state
            .source_base
            .saturating_add(source_text.len() as u64);
        let range = StreamRange::new(state.source_base, source_end);
        let builder = StreamSnapshotBuilder::new(
            state.revision,
            state.source_base,
            if state.sealed { source_end } else { state.source_base },
            source_end,
        );
        let builder = if source_text.is_empty() {
            builder.exact_text(range, [])
        } else {
            builder
                .atomic(range, View::text(state.display_text.clone()).into_view())
                .expect("host stream atomic view must be valid")
        };
        builder.finish().expect("host stream snapshot must be valid")
    }

    fn compact_before(&mut self, offset: StreamOffset) {
        let mut state = self.state.lock().expect("host stream lock is poisoned");
        let target = offset.min(
            state
                .source_base
                .saturating_add(state.source_text().len() as u64),
        );
        if target <= state.source_base {
            return;
        }
        let local = usize::try_from(target.as_u64() - state.source_base.as_u64())
            .expect("host stream coordinate fits usize");
        let source_text = state.source_text();
        if !source_text.is_char_boundary(local) {
            return;
        }
        let mut remaining = local;
        for segment in &mut state.segments {
            if remaining == 0 {
                break;
            }
            if remaining >= segment.text.len() {
                remaining -= segment.text.len();
                segment.text.clear();
            } else {
                segment.text.drain(..remaining);
                remaining = 0;
            }
        }
        state.segments.retain(|segment| !segment.text.is_empty());
        state.source_base = target;
        state.revision = state.revision.next();
        let _ = state.refresh_display();
    }

    fn seal(&mut self) {
        if let Ok(mut state) = self.state.lock() {
            if !state.sealed {
                state.sealed = true;
                state.revision = state.revision.next();
                let _ = state.refresh_display();
            }
        }
    }

    fn is_sealed(&self) -> bool {
        self.state
            .lock()
            .map(|state| state.sealed)
            .unwrap_or(true)
    }

    fn next_wakeup(&self) -> Option<Instant> {
        self.state.lock().ok().and_then(|state| state.next_wakeup())
    }

    fn advance(&mut self, now: Instant) -> bool {
        self.state
            .lock()
            .map(|mut state| state.advance(now))
            .unwrap_or(false)
    }

}

impl HostTextInput {
    pub fn new(multiline: bool) -> Self {
        Self {
            state: Arc::new(Mutex::new(TextInput::new().multiline(multiline))),
            component_id: Arc::new(Mutex::new(None)),
            host: Arc::new(Mutex::new(None)),
        }
    }

    pub fn text(&self) -> Result<String> {
        Ok(self.lock()?.text().to_owned())
    }

    pub fn cursor_bytes(&self) -> Result<usize> {
        Ok(self.lock()?.cursor_bytes())
    }

    pub fn set_text(&self, value: impl AsRef<str>) -> Result<()> {
        self.lock()?.set_text(value);
        self.render_host()
    }

    pub fn clear(&self) -> Result<()> {
        self.lock()?.clear();
        self.render_host()
    }

    pub fn submitted(&self) -> Result<Output<String>> {
        Ok(self.lock()?.submitted())
    }

    pub fn set_multiline(&self, enabled: bool) -> Result<()> {
        self.lock()?.set_multiline(enabled);
        self.render_host()
    }

    pub fn is_multiline(&self) -> Result<bool> {
        Ok(self.lock()?.is_multiline())
    }

    pub fn view(&self) -> Result<View> {
        Ok(self.lock()?.view())
    }

    pub fn component_id(&self) -> Option<u64> {
        self.component_id.lock().ok().and_then(|id| *id)
    }

    fn set_component_id(&self, id: u64) -> Result<()> {
        *self
            .component_id
            .lock()
            .map_err(|_| anyhow::anyhow!("text input component lock is poisoned"))? = Some(id);
        Ok(())
    }

    fn attach_host(&self, host: &Arc<Mutex<HostInner>>) -> Result<()> {
        *self
            .host
            .lock()
            .map_err(|_| anyhow::anyhow!("text input host lock is poisoned"))? =
            Some(Arc::downgrade(host));
        Ok(())
    }

    fn render_host(&self) -> Result<()> {
        let host = self
            .host
            .lock()
            .map_err(|_| anyhow::anyhow!("text input host lock is poisoned"))?
            .clone()
            .and_then(|host| host.upgrade());
        let Some(host) = host else {
            return Ok(());
        };
        let mut inner = host
            .lock()
            .map_err(|_| anyhow::anyhow!("host lock is poisoned"))?;
        inner.running.invalidate_frame();
        inner.render()
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, TextInput>> {
        self.state
            .lock()
            .map_err(|_| anyhow::anyhow!("text input lock is poisoned"))
    }
}

struct MountedTextInput(HostTextInput);

impl Component for MountedTextInput {
    fn view(&self) -> View {
        self.0
            .lock()
            .map(|input| input.view())
            .unwrap_or_else(|_| View::spacer(0))
    }

    fn capabilities(&self, cx: &mut ComponentCx<'_, Self>) {
        cx.focusable();
        cx.on_focus_changed(mounted_focus_changed);
        cx.key_commands(mounted_command_for_key, mounted_handle_command);
        cx.on_paste(mounted_paste);
        cx.on_layout_changed(mounted_layout_changed);
    }
}

fn mounted_command_for_key(
    component: &MountedTextInput,
    key: KeyStroke,
) -> Option<TextInputCommand> {
    component
        .0
        .lock()
        .ok()
        .and_then(|input| TextInput::command_for_key(&input, key))
}

fn mounted_handle_command(
    component: &mut MountedTextInput,
    command: TextInputCommand,
    cx: &mut crate::EventCx<'_>,
) -> InteractionResult {
    component
        .0
        .lock()
        .map(|mut input| TextInput::handle_command(&mut input, command, cx))
        .unwrap_or(InteractionResult::Ignored)
}

fn mounted_paste(
    component: &mut MountedTextInput,
    text: &str,
    cx: &mut crate::EventCx<'_>,
) -> InteractionResult {
    component
        .0
        .lock()
        .map(|mut input| TextInput::paste_callback(&mut input, text, cx))
        .unwrap_or(InteractionResult::Ignored)
}

fn mounted_focus_changed(component: &mut MountedTextInput, focused: bool) {
    if let Ok(mut input) = component.0.lock() {
        TextInput::focus_changed_callback(&mut input, focused);
    }
}

fn mounted_layout_changed(component: &mut MountedTextInput, size: Size) {
    if let Ok(mut input) = component.0.lock() {
        TextInput::layout_changed(&mut input, size);
    }
}

/// A handle to the History owned by a TuiHost.
#[derive(Clone)]
pub struct HostHistory {
    host: Arc<Mutex<HostInner>>,
}

impl HostHistory {
    pub fn layout(&self) -> Result<HistoryLayout> {
        let inner = self.lock()?;
        inner
            .running
            .scene_history()
            .map(History::layout)
            .ok_or_else(|| anyhow::anyhow!("host history is unavailable"))
    }

    pub fn set_layout(&self, layout: HistoryLayout) -> Result<()> {
        self.lock_mut()?
            .running
            .scene_history_mut()
            .ok_or_else(|| anyhow::anyhow!("host history is unavailable"))?
            .set_layout(layout);
        Ok(())
    }

    pub fn push(&self, view: View) -> Result<()> {
        let mut inner = self.lock_mut()?;
        inner
            .running
            .scene_history_mut()
            .ok_or_else(|| anyhow::anyhow!("host history is unavailable"))?
            .push(view)
            .map(|_| ())
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        inner.render()?;
        Ok(())
    }

    pub fn push_stream(&self, stream: &HostTextStream) -> Result<()> {
        let mut inner = self.lock_mut()?;
        stream.attach_host(&self.host)?;
        stream
            .attach(
                inner
            .running
            .scene_history_mut()
            .ok_or_else(|| anyhow::anyhow!("host history is unavailable"))?,
            )?;
        inner.render()?;
        Ok(())
    }

    pub fn seal_stream(&self, stream: &HostTextStream) -> Result<()> {
        let mut inner = self.lock_mut()?;
        let history = inner.running.scene_history_mut().ok_or_else(|| anyhow::anyhow!("host history is unavailable"))?;
        stream.seal_history(history)?;
        inner.render()?;
        Ok(())
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, HostInner>> {
        self.host
            .lock()
            .map_err(|_| anyhow::anyhow!("host lock is poisoned"))
    }

    fn lock_mut(&self) -> Result<std::sync::MutexGuard<'_, HostInner>> {
        self.lock()
    }
}

/// Native retained interaction host used by language bindings.
pub struct TuiHost {
    inner: Arc<Mutex<HostInner>>,
}

impl TuiHost {
    pub fn open(width: u16, height: u16, headless: bool) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(anyhow::anyhow!("terminal size must be positive"));
        }
        let backend = if headless {
            HostBackend::Headless(HeadlessSink {
                width,
                height,
                ..HeadlessSink::default()
            })
        } else {
            HostBackend::Real(TermwizBackend::enter()?)
        };
        let app = TuiApp::new(
            host_init as fn(&mut AppCx<'_, HostAction>) -> Result<HostState>,
            host_update
                as fn(&mut HostState, HostAction, &mut AppCx<'_, HostAction>) -> Result<()>,
            host_view as fn(&HostState) -> View,
        )
        .with_theme(Theme::new())
        .with_history(History::new().with_layout(HistoryLayout::from_parts(
            crate::Insets::new(0, 0, 1, 0),
            1,
        )));
        let now = Instant::now();
        let mut running = app.start(now).map_err(|error| anyhow::anyhow!("host init failed: {error:?}"))?;
        let mut backend = backend;
        let frame = prepare_frame(&mut running, &mut backend, now)?;
        let inner = Arc::new(Mutex::new(HostInner {
            running,
            backend,
            frame,
            now,
            headless,
            closed: false,
        }));
        Ok(Self { inner })
    }

    pub fn history(&self) -> HostHistory {
        HostHistory {
            host: Arc::clone(&self.inner),
        }
    }

    pub fn create_text_input(&self, multiline: bool) -> Result<HostTextInput> {
        let input = HostTextInput::new(multiline);
        input.lock()?.set_border(BorderSpec::plain().edges(BorderEdges::TOP_BOTTOM));
        input.attach_host(&self.inner)?;
        let mut inner = self.lock_mut()?;
        let handle = inner
            .running
            .host_register(MountedTextInput(input.clone()));
        input.set_component_id(handle.raw_id())?;
        Ok(input)
    }

    pub fn create_working(&self, config: HostActivityConfig) -> Result<HostWorking> {
        let working = HostWorking::new(config);
        let mut inner = self.lock_mut()?;
        let handle = inner.running.host_register(MountedWorking(working.clone()));
        working.set_component_id(handle.raw_id())?;
        Ok(working)
    }

    pub fn create_view_slot(&self, view: View) -> Result<HostViewSlot> {
        let slot = HostViewSlot::new(view);
        slot.attach_host(&self.inner)?;
        let mut inner = self.lock_mut()?;
        let handle = inner.running.host_register(MountedViewSlot(slot.clone()));
        slot.set_component_id(handle.raw_id())?;
        Ok(slot)
    }

    pub fn bind_key(&self, key: KeyStroke, action_id: impl Into<String>) -> Result<()> {
        let action_id = action_id.into();
        self.lock_mut()?.running.host_bind_key(key, move || HostAction::Routed(RoutedAction {
            action_id: action_id.clone(),
            payload: None,
        }));
        Ok(())
    }

    pub fn exit(&self) -> Result<()> {
        let mut inner = self.lock_mut()?;
        inner.running.host_exit();
        Ok(())
    }

    pub fn route_text_input(
        &self,
        input: &HostTextInput,
        action_id: impl Into<String>,
    ) -> Result<()> {
        let output = input.submitted()?;
        let action_id = action_id.into();
        self.lock_mut()?
            .running
            .host_route(output, move |text| HostAction::Routed(RoutedAction {
                action_id: action_id.clone(),
                payload: Some(text),
            }))
            .map_err(|_| anyhow::anyhow!("output route already exists"))?;
        Ok(())
    }

    pub fn route_text_input_output(
        &self,
        output: Output<String>,
        action_id: impl Into<String>,
    ) -> Result<()> {
        let action_id = action_id.into();
        self.lock_mut()?
            .running
            .host_route(output, move |text| HostAction::Routed(RoutedAction {
                action_id: action_id.clone(),
                payload: Some(text),
            }))
            .map_err(|_| anyhow::anyhow!("output route already exists"))?;
        Ok(())
    }

    pub fn intercept_paste(
        &self,
        input: &HostTextInput,
        action_id: impl Into<String>,
    ) -> Result<()> {
        let id = input
            .component_id()
            .ok_or_else(|| anyhow::anyhow!("text input is not mounted"))?;
        let handle = ComponentHandle::<MountedTextInput>::from_raw_id(id);
        let action_id = action_id.into();
        self.lock_mut()?.running.host_intercept_paste(handle, move |text| {
            HostAction::Routed(RoutedAction {
                action_id: action_id.clone(),
                payload: Some(text),
            })
        });
        Ok(())
    }

    pub fn render(&self, body: View) -> Result<()> {
        let mut inner = self.lock_mut()?;
        inner.running.host_set_body(body);
        inner.render()
    }

    pub fn set_theme(&self, theme: Theme) -> Result<()> {
        let mut inner = self.lock_mut()?;
        inner.running.host_set_theme(theme);
        inner.render()
    }

    pub fn set_history(&self, history: History) -> Result<()> {
        let mut inner = self.lock_mut()?;
        inner.running.host_set_history(history);
        inner.render()
    }

    pub fn dispatch_key(&self, key: KeyStroke) -> Result<()> {
        let mut inner = self.lock_mut()?;
        inner.running.dispatch_key(key).map_err(|error| anyhow::anyhow!("key dispatch failed: {error:?}"))?;
        inner.advance_and_render()
    }

    pub fn dispatch_paste(&self, text: &str) -> Result<()> {
        let mut inner = self.lock_mut()?;
        inner.running.dispatch_paste(text).map_err(|error| anyhow::anyhow!("paste dispatch failed: {error:?}"))?;
        inner.advance_and_render()
    }

    pub fn resize(&self, width: u16, height: u16) -> Result<()> {
        if width == 0 || height == 0 {
            return Err(anyhow::anyhow!("terminal size must be positive"));
        }
        let mut inner = self.lock_mut()?;
        if let HostBackend::Headless(sink) = &mut inner.backend {
            sink.width = width;
            sink.height = height;
        }
        inner.running.invalidate_frame();
        inner.now = Instant::now();
        inner.advance_and_render()
    }

    pub fn advance_time(&self, duration: Duration) -> Result<()> {
        let mut inner = self.lock_mut()?;
        inner.now += duration;
        inner.advance_and_render()
    }

    pub fn next_action(&self) -> Option<RoutedAction> {
        self.lock_mut().ok()?.running.state.actions.pop_front()
    }

    pub fn style_at(&self, row: u16, column: u16) -> Option<HostCellStyle> {
        let inner = self.lock().ok()?;
        if row >= inner.frame.surface.height() || column >= inner.frame.surface.width() {
            return None;
        }
        let style = inner.frame.surface.get(column, row).style;
        Some(HostCellStyle {
            foreground: style.foreground.map(physical_color),
            background: style.background.map(physical_color),
            bold: style.bold,
            dim: style.dim,
            italic: style.italic,
            underline: style.underline,
            reversed: style.reversed,
            strikethrough: style.strikethrough,
        })
    }

    pub fn cell_x_of_text(&self, row: u16, needle: &str) -> Option<u16> {
        let inner = self.lock().ok()?;
        if row >= inner.frame.surface.height() {
            return None;
        }
        if needle.is_empty() {
            return Some(0);
        }
        for start in 0..inner.frame.surface.width() {
            if inner.frame.surface.get(start, row).continuation {
                continue;
            }
            let mut candidate = String::new();
            for column in start..inner.frame.surface.width() {
                let cell = inner.frame.surface.get(column, row);
                if cell.continuation {
                    continue;
                }
                candidate.push_str(cell.grapheme.as_deref().unwrap_or(" "));
                if candidate == needle {
                    return Some(start);
                }
                if !needle.starts_with(&candidate) {
                    break;
                }
            }
        }
        None
    }

    pub fn exited(&self) -> bool {
        self.lock().map(|inner| inner.closed || inner.running.host_exited()).unwrap_or(true)
    }

    pub fn poll_terminal(&self) -> Result<()> {
        let mut inner = self.lock_mut()?;
        let event = match &mut inner.backend {
            HostBackend::Headless(_) => None,
            HostBackend::Real(backend) => backend.try_next_event()?,
        };
        match event {
            Some(TerminalEvent::Key(key)) => {
                inner.running.dispatch_key(key).map_err(|error| anyhow::anyhow!("key dispatch failed: {error:?}"))?;
                inner.advance_and_render()
            }
            Some(TerminalEvent::Paste(text)) => {
                inner.running.dispatch_paste(&text).map_err(|error| anyhow::anyhow!("paste dispatch failed: {error:?}"))?;
                inner.advance_and_render()
            }
            Some(TerminalEvent::Resize) => {
                inner.running.invalidate_frame();
                inner.advance_and_render()
            }
            None => inner.advance_and_render(),
        }
    }

    pub fn screen_rows(&self) -> Vec<String> {
        self.lock().map(|inner| inner.frame.screen_lines()).unwrap_or_default()
    }

    pub fn native_history_rows(&self) -> Vec<String> {
        self.lock()
            .ok()
            .and_then(|inner| match &inner.backend {
                HostBackend::Headless(sink) => Some(sink.history.iter().map(PhysicalRow::plain_text).collect()),
                HostBackend::Real(_) => Some(Vec::new()),
            })
            .unwrap_or_default()
    }

    pub fn close(&self) -> Result<()> {
        let mut inner = self.lock_mut()?;
        if inner.closed {
            return Ok(());
        }
        inner.closed = true;
        if let HostBackend::Real(backend) = &mut inner.backend {
            backend.restore()?;
        }
        Ok(())
    }

    pub fn is_headless(&self) -> bool {
        self.lock().map(|inner| inner.headless).unwrap_or(true)
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, HostInner>> {
        self.inner
            .lock()
            .map_err(|_| anyhow::anyhow!("host lock is poisoned"))
    }

    fn lock_mut(&self) -> Result<std::sync::MutexGuard<'_, HostInner>> {
        self.lock()
    }
}

fn physical_color(color: crate::physical::PhysicalColor) -> String {
    match color {
        crate::physical::PhysicalColor::Default => "default".to_owned(),
        crate::physical::PhysicalColor::Named(color) => format!("{color:?}"),
        crate::physical::PhysicalColor::Indexed(value) => format!("ansi:{value}"),
        crate::physical::PhysicalColor::Rgb { r, g, b } => format!("#{r:02x}{g:02x}{b:02x}"),
    }
}

impl Drop for TuiHost {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

impl HostInner {
    fn render(&mut self) -> Result<()> {
        self.frame = prepare_frame(&mut self.running, &mut self.backend, self.now)?;
        Ok(())
    }

    fn advance_and_render(&mut self) -> Result<()> {
        let status = self
            .running
            .advance_ready(self.now)
            .map_err(|error| anyhow::anyhow!("host update failed: {error:?}"))?;
        if status.dirty {
            self.render()?;
        }
        Ok(())
    }

}

fn prepare_frame(running: &mut HostRunning, backend: &mut HostBackend, now: Instant) -> Result<PreparedSceneFrame> {
    match backend {
        HostBackend::Headless(sink) => running
            .prepare_frame(now, sink, |sink| Ok(Size::new(sink.width, sink.height)))
            .map_err(|error| anyhow::anyhow!("headless render failed: {error:?}")),
        HostBackend::Real(backend) => {
            let frame = running
                .prepare_frame(now, backend, |backend| backend.viewport())
                .map_err(|error| anyhow::anyhow!("terminal render failed: {error:?}"))?;
            backend
                .begin_frame(&frame)?
                .blocking_recv()
                .map_err(|error| anyhow::anyhow!("terminal presentation reply lost: {error}"))??;
            Ok(frame)
        }
    }
}

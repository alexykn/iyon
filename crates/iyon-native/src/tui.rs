use napi::bindgen_prelude::Result;
use napi_derive::napi;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use iyon_tui::{BorderEdges, BorderSpec, Component, History, HostHistory, HostTextInput, HostTextStream, HostWorking, IntoView, Key, KeyStroke, Modifiers, Output, TextInput, TuiHost, View};
use serde_json::Map;
use serde_json::Value;

/// Link/surface probe only: construct one owned public TUI value and discard
/// it. The native bridge must not duplicate or serialize the TUI renderer.
#[napi(js_name = "tuiSmoke")]
pub fn tui_smoke() -> Result<String> {
    let _view = View::text("iyon-tui/t1").into_view();
    Ok("iyon-tui/t1".to_owned())
}

/// Opaque native result of one semantic materialization boundary. The Rust
/// View remains owned by the native object; JavaScript only receives the
/// validated handle object created by N-API.
#[napi]
pub struct NativeTuiView {
    #[allow(dead_code)]
    view: View,
}

#[napi]
pub struct NativeTuiOutput {
    output: Output<String>,
}

fn ensure_alive(alive: &AtomicBool) -> Result<()> {
    if alive.load(Ordering::Acquire) {
        return Ok(());
    }
    Err(crate::NativeError::coded(
        napi::Status::Closing,
        "ION_DISPOSED_HANDLE",
        "native TUI handle has been disposed",
    ))
}

#[napi]
pub struct NativeHistory {
    state: Mutex<History>,
    host: Option<HostHistory>,
    alive: AtomicBool,
}

#[napi]
impl NativeHistory {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            state: Mutex::new(History::new()),
            host: None,
            alive: AtomicBool::new(true),
        }
    }

    #[napi]
    pub fn dispose(&self) {
        self.alive.store(false, Ordering::Release);
    }

    #[napi]
    pub fn layout(&self) -> Result<Value> {
        ensure_alive(&self.alive)?;
        if let Some(host) = &self.host {
            let layout = host
                .layout()
                .map_err(|error| crate::NativeError::internal(error.to_string()))?;
            return Ok(serde_json::json!({"padding": layout.padding().bottom(), "gap": layout.gap()}));
        }
        let _layout = self
            .state
            .lock()
            .map_err(|_| crate::NativeError::internal("history lock is poisoned"))?
            .layout();
        Ok(serde_json::json!({"padding": _layout.padding().bottom(), "gap": _layout.gap()}))
    }

    #[napi]
    pub fn push(&self, view: &NativeTuiView) -> Result<()> {
        ensure_alive(&self.alive)?;
        if let Some(host) = &self.host {
            return host
                .push(view.view.clone())
                .map_err(|error| crate::NativeError::invalid_input(error.to_string()));
        }
        self.state
            .lock()
            .map_err(|_| crate::NativeError::internal("history lock is poisoned"))?
            .push(view.view.clone())
            .map(|_| ())
            .map_err(|error| crate::NativeError::invalid_input(error.to_string()))
    }

    fn from_host(host: HostHistory) -> Self {
        Self {
            state: Mutex::new(History::new()),
            host: Some(host),
            alive: AtomicBool::new(true),
        }
    }

    #[napi(js_name = "pushStream")]
    pub fn push_stream(&self, stream: &NativeTextStream) -> Result<()> {
        ensure_alive(&self.alive)?;
        if let Some(host) = &self.host {
            return host
                .push_stream(&stream.stream)
                .map_err(|error| crate::NativeError::invalid_input(error.to_string()));
        }
        let mut history = self
            .state
            .lock()
            .map_err(|_| crate::NativeError::internal("history lock is poisoned"))?;
        stream
            .stream
            .attach(&mut history)
            .map_err(|error| crate::NativeError::invalid_input(error.to_string()))
    }
}

#[napi]
pub struct NativeTextInput {
    state: Mutex<TextInput>,
    host: Option<HostTextInput>,
    alive: AtomicBool,
}

#[napi]
pub struct NativeWorking {
    working: HostWorking,
    alive: AtomicBool,
}

#[napi]
impl NativeWorking {
    #[napi]
    pub fn dispose(&self) {
        self.alive.store(false, Ordering::Release);
    }

    #[napi(js_name = "componentId")]
    pub fn component_id(&self) -> Result<Option<i64>> {
        ensure_alive(&self.alive)?;
        Ok(self.working.component_id().map(|id| id as i64))
    }

    #[napi(js_name = "setActive")]
    pub fn set_active(&self, active: bool) -> Result<()> {
        ensure_alive(&self.alive)?;
        self.working
            .set_active(active)
            .map_err(|error| crate::NativeError::internal(error.to_string()))
    }

    #[napi(js_name = "setPending")]
    pub fn set_pending(&self, pending: Vec<String>) -> Result<()> {
        ensure_alive(&self.alive)?;
        self.working
            .set_pending(pending)
            .map_err(|error| crate::NativeError::internal(error.to_string()))
    }
}

#[napi]
impl NativeTextInput {
    #[napi(constructor)]
    pub fn new(multiline: Option<bool>) -> Self {
        Self {
            state: Mutex::new(TextInput::new().multiline(multiline.unwrap_or(false))),
            host: None,
            alive: AtomicBool::new(true),
        }
    }

    #[napi]
    pub fn dispose(&self) {
        self.alive.store(false, Ordering::Release);
    }

    #[napi]
    pub fn text(&self) -> Result<String> {
        ensure_alive(&self.alive)?;
        if let Some(host) = &self.host {
            return host
                .text()
                .map_err(|error| crate::NativeError::internal(error.to_string()));
        }
        Ok(self
            .state
            .lock()
            .map_err(|_| crate::NativeError::internal("text input lock is poisoned"))?
            .text()
            .to_owned())
    }

    #[napi(js_name = "cursorBytes")]
    pub fn cursor_bytes(&self) -> Result<i64> {
        ensure_alive(&self.alive)?;
        if let Some(host) = &self.host {
            return host
                .cursor_bytes()
                .map(|cursor| cursor as i64)
                .map_err(|error| crate::NativeError::internal(error.to_string()));
        }
        Ok(self
            .state
            .lock()
            .map_err(|_| crate::NativeError::internal("text input lock is poisoned"))?
            .cursor_bytes() as i64)
    }

    #[napi(js_name = "setText")]
    pub fn set_text(&self, text: String) -> Result<()> {
        ensure_alive(&self.alive)?;
        if let Some(host) = &self.host {
            return host
                .set_text(text)
                .map_err(|error| crate::NativeError::internal(error.to_string()));
        }
        self.state
            .lock()
            .map_err(|_| crate::NativeError::internal("text input lock is poisoned"))?
            .set_text(text);
        Ok(())
    }

    #[napi]
    pub fn clear(&self) -> Result<()> {
        ensure_alive(&self.alive)?;
        if let Some(host) = &self.host {
            return host
                .clear()
                .map_err(|error| crate::NativeError::internal(error.to_string()));
        }
        self.state
            .lock()
            .map_err(|_| crate::NativeError::internal("text input lock is poisoned"))?
            .clear();
        Ok(())
    }

    #[napi(js_name = "setMultiline")]
    pub fn set_multiline(&self, enabled: bool) -> Result<()> {
        ensure_alive(&self.alive)?;
        if let Some(host) = &self.host {
            return host
                .set_multiline(enabled)
                .map_err(|error| crate::NativeError::internal(error.to_string()));
        }
        self.state
            .lock()
            .map_err(|_| crate::NativeError::internal("text input lock is poisoned"))?
            .set_multiline(enabled);
        Ok(())
    }

    #[napi(js_name = "isMultiline")]
    pub fn is_multiline(&self) -> Result<bool> {
        ensure_alive(&self.alive)?;
        if let Some(host) = &self.host {
            return host
                .is_multiline()
                .map_err(|error| crate::NativeError::internal(error.to_string()));
        }
        Ok(self
            .state
            .lock()
            .map_err(|_| crate::NativeError::internal("text input lock is poisoned"))?
            .is_multiline())
    }

    #[napi]
    pub fn submitted(&self) -> Result<NativeTuiOutput> {
        ensure_alive(&self.alive)?;
        let output = if let Some(host) = &self.host {
            host.submitted()
                .map_err(|error| crate::NativeError::internal(error.to_string()))?
        } else {
            self.state
                .lock()
                .map_err(|_| crate::NativeError::internal("text input lock is poisoned"))?
                .submitted()
        };
        Ok(NativeTuiOutput { output })
    }

    #[napi]
    pub fn view(&self) -> Result<NativeTuiView> {
        ensure_alive(&self.alive)?;
        if let Some(host) = &self.host {
            return host
                .view()
                .map(|view| NativeTuiView { view })
                .map_err(|error| crate::NativeError::internal(error.to_string()));
        }
        let input = self
            .state
            .lock()
            .map_err(|_| crate::NativeError::internal("text input lock is poisoned"))?;
        Ok(NativeTuiView { view: input.view() })
    }

    #[napi(js_name = "componentId")]
    pub fn component_id(&self) -> Result<Option<i64>> {
        ensure_alive(&self.alive)?;
        Ok(self
            .host
            .as_ref()
            .and_then(HostTextInput::component_id)
            .map(|id| id as i64))
    }

    fn from_host(host: HostTextInput) -> Self {
        Self {
            state: Mutex::new(TextInput::new()),
            host: Some(host),
            alive: AtomicBool::new(true),
        }
    }
}

#[napi]
pub struct NativeTuiHost {
    host: TuiHost,
    alive: AtomicBool,
}

#[napi]
impl NativeTuiHost {
    #[napi(constructor)]
    pub fn new(width: Option<i64>, height: Option<i64>, headless: Option<bool>) -> Result<Self> {
        let width = width.unwrap_or(80);
        let height = height.unwrap_or(24);
        let width = u16::try_from(width).map_err(|_| crate::NativeError::invalid_input("width must fit in u16"))?;
        let height = u16::try_from(height).map_err(|_| crate::NativeError::invalid_input("height must fit in u16"))?;
        let host = TuiHost::open(width, height, headless.unwrap_or(false))
            .map_err(|error| crate::NativeError::internal(error.to_string()))?;
        Ok(Self { host, alive: AtomicBool::new(true) })
    }

    #[napi]
    pub fn dispose(&self) -> Result<()> {
        if self.alive.swap(false, Ordering::AcqRel) {
            self.host.close().map_err(|error| crate::NativeError::internal(error.to_string()))?;
        }
        Ok(())
    }

    #[napi]
    pub fn exit(&self) -> Result<()> {
        ensure_alive(&self.alive)?;
        self.host
            .exit()
            .map_err(|error| crate::NativeError::internal(error.to_string()))
    }

    #[napi]
    pub fn history(&self) -> Result<NativeHistory> {
        ensure_alive(&self.alive)?;
        Ok(NativeHistory::from_host(self.host.history()))
    }

    #[napi(js_name = "textInput")]
    pub fn text_input(&self, multiline: Option<bool>) -> Result<NativeTextInput> {
        ensure_alive(&self.alive)?;
        let input = self.host.create_text_input(multiline.unwrap_or(false))
            .map_err(|error| crate::NativeError::internal(error.to_string()))?;
        Ok(NativeTextInput::from_host(input))
    }

    #[napi(js_name = "working")]
    pub fn working(&self) -> Result<NativeWorking> {
        ensure_alive(&self.alive)?;
        let working = self.host.create_working()
            .map_err(|error| crate::NativeError::internal(error.to_string()))?;
        Ok(NativeWorking { working, alive: AtomicBool::new(true) })
    }

    #[napi(js_name = "bindKey")]
    pub fn bind_key(&self, key: String, modifiers: Option<Vec<String>>, action_id: String) -> Result<()> {
        ensure_alive(&self.alive)?;
        self.host.bind_key(parse_key(&key, modifiers.as_deref())?, action_id)
            .map_err(|error| crate::NativeError::invalid_input(error.to_string()))
    }

    #[napi]
    pub fn route(&self, output: &NativeTuiOutput, action_id: String) -> Result<()> {
        ensure_alive(&self.alive)?;
        self.host.route_text_input_output(output.output, action_id)
            .map_err(|error| crate::NativeError::invalid_input(error.to_string()))
    }

    #[napi(js_name = "interceptPaste")]
    pub fn intercept_paste(&self, input: &NativeTextInput, action_id: String) -> Result<()> {
        ensure_alive(&self.alive)?;
        let host_input = input.host.as_ref().ok_or_else(|| crate::NativeError::invalid_input("text input is not mounted"))?;
        self.host.intercept_paste(host_input, action_id)
            .map_err(|error| crate::NativeError::invalid_input(error.to_string()))
    }

    #[napi]
    pub fn render(&self, view: &NativeTuiView) -> Result<()> {
        ensure_alive(&self.alive)?;
        self.host.render(view.view.clone()).map_err(|error| crate::NativeError::internal(error.to_string()))
    }

    #[napi(js_name = "dispatchKey")]
    pub fn dispatch_key(&self, key: String, modifiers: Option<Vec<String>>) -> Result<()> {
        ensure_alive(&self.alive)?;
        self.host.dispatch_key(parse_key(&key, modifiers.as_deref())?)
            .map_err(|error| crate::NativeError::internal(error.to_string()))
    }

    #[napi(js_name = "dispatchPaste")]
    pub fn dispatch_paste(&self, text: String) -> Result<()> {
        ensure_alive(&self.alive)?;
        self.host.dispatch_paste(&text).map_err(|error| crate::NativeError::internal(error.to_string()))
    }

    #[napi(js_name = "pollTerminal")]
    pub fn poll_terminal(&self) -> Result<()> {
        ensure_alive(&self.alive)?;
        self.host.poll_terminal().map_err(|error| crate::NativeError::internal(error.to_string()))
    }

    #[napi(js_name = "nextAction")]
    pub fn next_action(&self) -> Result<Option<Value>> {
        ensure_alive(&self.alive)?;
        Ok(self.host.next_action().map(|action| serde_json::json!({"action_id": action.action_id, "payload": action.payload})))
    }

    #[napi(js_name = "screenRows")]
    pub fn screen_rows(&self) -> Result<Vec<String>> {
        ensure_alive(&self.alive)?;
        Ok(self.host.screen_rows())
    }

    #[napi(js_name = "nativeHistoryRows")]
    pub fn native_history_rows(&self) -> Result<Vec<String>> {
        ensure_alive(&self.alive)?;
        Ok(self.host.native_history_rows())
    }

    #[napi]
    pub fn resize(&self, width: i64, height: i64) -> Result<()> {
        ensure_alive(&self.alive)?;
        let width = u16::try_from(width).map_err(|_| crate::NativeError::invalid_input("width must fit in u16"))?;
        let height = u16::try_from(height).map_err(|_| crate::NativeError::invalid_input("height must fit in u16"))?;
        self.host.resize(width, height).map_err(|error| crate::NativeError::internal(error.to_string()))
    }

    #[napi(js_name = "advanceTime")]
    pub fn advance_time(&self, milliseconds: i64) -> Result<()> {
        ensure_alive(&self.alive)?;
        let milliseconds = u64::try_from(milliseconds).map_err(|_| crate::NativeError::invalid_input("time must be non-negative"))?;
        self.host.advance_time(std::time::Duration::from_millis(milliseconds)).map_err(|error| crate::NativeError::internal(error.to_string()))
    }
}

fn parse_key(key: &str, modifiers: Option<&[String]>) -> Result<KeyStroke> {
    let key = match key {
        "Enter" => Key::Enter,
        "Escape" => Key::Escape,
        "Backspace" => Key::Backspace,
        "Tab" => Key::Tab,
        "Delete" => Key::Delete,
        "Insert" => Key::Insert,
        "Home" => Key::Home,
        "End" => Key::End,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        "Up" => Key::Up,
        "Down" => Key::Down,
        "Left" => Key::Left,
        "Right" => Key::Right,
        value => {
            let mut chars = value.chars();
            let Some(character) = chars.next() else { return Err(crate::NativeError::invalid_input("key must not be empty")); };
            if chars.next().is_some() { return Err(crate::NativeError::invalid_input("character key must contain one character")); }
            Key::Char(character)
        }
    };
    let mut flags = Modifiers::NONE;
    for modifier in modifiers.unwrap_or_default() {
        flags = flags.union(match modifier.to_ascii_lowercase().as_str() {
            "shift" => Modifiers::SHIFT,
            "control" | "ctrl" => Modifiers::CONTROL,
            "alt" | "option" => Modifiers::ALT,
            "super" | "meta" => Modifiers::SUPER,
            other => return Err(crate::NativeError::invalid_input(format!("unknown key modifier `{other}`"))),
        });
    }
    Ok(KeyStroke::with_modifiers(key, flags))
}

#[napi]
pub struct NativeTextStream {
    stream: HostTextStream,
    alive: AtomicBool,
}

#[napi]
impl NativeTextStream {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            stream: HostTextStream::new(),
            alive: AtomicBool::new(true),
        }
    }

    #[napi]
    pub fn dispose(&self) {
        self.alive.store(false, Ordering::Release);
    }

    #[napi]
    pub fn update(&self, text: String) -> Result<()> {
        ensure_alive(&self.alive)?;
        self.stream
            .update(text)
            .map_err(|error| crate::NativeError::invalid_input(error.to_string()))
    }

    #[napi]
    pub fn seal(&self) -> Result<()> {
        ensure_alive(&self.alive)?;
        self.stream
            .seal()
            .map_err(|error| crate::NativeError::invalid_input(error.to_string()))
    }

    #[napi]
    pub fn snapshot(&self) -> Result<Value> {
        ensure_alive(&self.alive)?;
        let (text, revision, sealed) = self
            .stream
            .snapshot_json()
            .map_err(|error| crate::NativeError::internal(error.to_string()))?;
        Ok(serde_json::json!({"text": text, "revision": revision, "sealed": sealed}))
    }

}

#[napi]
pub struct NativeComponent {
    id: u64,
    revision: AtomicU64,
    alive: AtomicBool,
}

#[napi]
impl NativeComponent {
    #[napi(constructor)]
    pub fn new() -> Self {
        static NEXT_COMPONENT_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            id: NEXT_COMPONENT_ID.fetch_add(1, Ordering::AcqRel),
            revision: AtomicU64::new(0),
            alive: AtomicBool::new(true),
        }
    }

    #[napi]
    pub fn dispose(&self) {
        self.alive.store(false, Ordering::Release);
    }

    #[napi]
    pub fn revision(&self) -> Result<i64> {
        ensure_alive(&self.alive)?;
        Ok(self.revision.load(Ordering::Acquire) as i64)
    }

    #[napi]
    pub fn id(&self) -> Result<i64> {
        ensure_alive(&self.alive)?;
        Ok(self.id as i64)
    }
}

#[napi(js_name = "materializeView")]
pub fn materialize_view(value: Value) -> Result<NativeTuiView> {
    let view = lower_view(&value)?;
    Ok(NativeTuiView { view })
}

fn lower_view(value: &Value) -> Result<View> {
    let object = value
        .as_object()
        .ok_or_else(|| crate::NativeError::invalid_input("view node must be an object"))?;
    let kind = object
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| crate::NativeError::invalid_input("view node type must be a string"))?;
    let view = match kind {
        "text" => {
            let spans = object
                .get("spans")
                .and_then(Value::as_array)
                .ok_or_else(|| crate::NativeError::invalid_input("text spans must be an array"))?;
            let text = spans
                .iter()
                .map(|span| {
                    span.as_object()
                        .and_then(|span| span.get("text"))
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            crate::NativeError::invalid_input("text span text must be a string")
                        })
                })
                .collect::<Result<Vec<_>>>()?
                .join("");
            View::text(text).into_view()
        }
        "spacer" => {
            let rows = u16_value(object, "rows")?;
            View::spacer(rows)
        }
        "row" => {
            let children = child_views(object)?;
            View::horizontal(|row| {
                row.children(children);
            })
        }
        "column" => {
            let children = object
                .get("children")
                .and_then(Value::as_array)
                .ok_or_else(|| crate::NativeError::invalid_input("view children must be an array"))?;
            let mut lowered = Vec::with_capacity(children.len());
            for child in children {
                if child.get("type").and_then(Value::as_str) == Some("contentMax") {
                    let max = child
                        .get("maxRows")
                        .and_then(Value::as_u64)
                        .and_then(|value| u16::try_from(value).ok())
                        .ok_or_else(|| crate::NativeError::invalid_input("contentMax maxRows must fit in u16"))?;
                    let nested = lower_view(child.get("child").ok_or_else(|| crate::NativeError::invalid_input("contentMax child is required"))?)?;
                    lowered.push((max, nested));
                } else {
                    let view = lower_view(child)?;
                    lowered.push((0, view));
                }
            }
            View::vertical(|column| {
                for (max, view) in lowered {
                    if max == 0 {
                        column.child(view);
                    } else {
                        column.content_max(max, view);
                    }
                }
            })
        }
        "hanging" => View::hanging(
            lower_required(object, "prefix")?,
            lower_required(object, "continuation")?,
            lower_required(object, "body")?,
        ),
        "grid" => {
            // Grid track metadata is intentionally retained for the native
            // lowering seam; an empty-track grid is still a valid semantic
            // view and children are lowered through the canonical API.
            let children = child_views(object)?;
            View::vertical(|column| {
                column.children(children);
            })
        }
        "container" => lower_required(object, "child")?.container(),
        "clamp" => lower_required(object, "child")?.clamp_rows(
            u16_value(object, "maxRows")?,
            iyon_tui::OverflowIndicator::None,
        ),
        "decorated" => {
            apply_decoration(lower_required(object, "child")?, object.get("decoration"))?
        }
        "component" => View::native_component(
            object
                .get("handle")
                .and_then(Value::as_u64)
                .ok_or_else(|| crate::NativeError::invalid_input("component handle must be an integer"))?,
        ),
        "contentMax" => lower_required(object, "child")?.clamp_rows(
            u16_value(object, "maxRows")?,
            iyon_tui::OverflowIndicator::None,
        ),
        other => {
            return Err(crate::NativeError::invalid_input(format!(
                "unknown view node type `{other}`"
            )));
        }
    };
    Ok(view)
}

fn child_views(object: &Map<String, Value>) -> Result<Vec<View>> {
    object
        .get("children")
        .and_then(Value::as_array)
        .ok_or_else(|| crate::NativeError::invalid_input("view children must be an array"))?
        .iter()
        .map(lower_view)
        .collect()
}

fn lower_required(object: &Map<String, Value>, field: &str) -> Result<View> {
    lower_view(object.get(field).ok_or_else(|| {
        crate::NativeError::invalid_input(format!("view node field `{field}` is required"))
    })?)
}

fn u16_value(object: &Map<String, Value>, field: &str) -> Result<u16> {
    let value = object
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| crate::NativeError::invalid_input(format!("{field} must be an integer")))?;
    u16::try_from(value)
        .map_err(|_| crate::NativeError::invalid_input(format!("{field} must fit in u16")))
}

fn apply_decoration(view: View, decoration: Option<&Value>) -> Result<View> {
    let Some(decoration) = decoration.and_then(Value::as_object) else {
        return Ok(view);
    };
    let mut view = view;
    if let Some(value) = decoration.get("padding") {
        let padding = value
            .as_object()
            .ok_or_else(|| crate::NativeError::invalid_input("padding must be an object"))?;
        view = view.padding(iyon_tui::Insets::new(
            u16_value(padding, "top")?,
            u16_value(padding, "right")?,
            u16_value(padding, "bottom")?,
            u16_value(padding, "left")?,
        ));
    }
    if let Some(color) = decoration.get("background").and_then(color_spec) {
        view = view.background(color);
    }
    if let Some(color) = decoration.get("foreground").and_then(color_spec) {
        view = view.foreground(color);
    }
    if let Some(border) = decoration.get("border").and_then(Value::as_object) {
        let mut spec = match border.get("style").and_then(Value::as_str).unwrap_or("plain") {
            "plain" => BorderSpec::plain(),
            "rounded" => BorderSpec::rounded(),
            "double" => BorderSpec::double(),
            other => return Err(crate::NativeError::invalid_input(format!("unknown border style `{other}`"))),
        };
        if border.get("edges").and_then(Value::as_str) == Some("topBottom") {
            spec = spec.edges(BorderEdges::TOP_BOTTOM);
        }
        if let Some(color) = border.get("color").and_then(color_spec) {
            spec = spec.color(color);
        }
        view = view.border(spec);
    }
    if let Some(style) = decoration.get("style").and_then(Value::as_object) {
        if let Some(attributes) = style.get("attributes").and_then(Value::as_object) {
            for (name, enabled) in attributes {
                if let Some(attribute) = text_attribute(name) {
                    view = view.text_attribute(attribute, enabled.as_bool().unwrap_or(false));
                }
            }
        }
    }
    if let Some(states) = decoration.get("styleStates").and_then(Value::as_object) {
        for (key, value) in states {
            let value = value
                .as_str()
                .ok_or_else(|| crate::NativeError::invalid_input("style state values must be strings"))?;
            view = view.style_state(key.as_str(), value);
        }
    }
    if decoration.get("width").and_then(Value::as_str) == Some("fill") {
        view = view.fill_width();
    }
    if decoration.get("height").and_then(Value::as_str) == Some("fill") {
        view = view.fill_height();
    }
    Ok(view)
}

fn color_spec(value: &Value) -> Option<iyon_tui::ColorSpec> {
    let value = value.as_str()?;
    if let Some(value) = value.strip_prefix("theme:") {
        return Some(iyon_tui::ColorSpec::theme(value));
    }
    if let Some(value) = value.strip_prefix("ansi:") {
        return value.parse::<u8>().ok().map(iyon_tui::ColorSpec::ansi);
    }
    if let Some(value) = value.strip_prefix('#') {
        if value.len() == 6 {
            let r = u8::from_str_radix(&value[0..2], 16).ok()?;
            let g = u8::from_str_radix(&value[2..4], 16).ok()?;
            let b = u8::from_str_radix(&value[4..6], 16).ok()?;
            return Some(iyon_tui::ColorSpec::rgb(r, g, b));
        }
    }
    let color = match value.to_ascii_lowercase().as_str() {
        "black" => iyon_tui::AnsiColor::Black,
        "red" => iyon_tui::AnsiColor::Red,
        "green" => iyon_tui::AnsiColor::Green,
        "yellow" => iyon_tui::AnsiColor::Yellow,
        "blue" => iyon_tui::AnsiColor::Blue,
        "magenta" => iyon_tui::AnsiColor::Magenta,
        "cyan" => iyon_tui::AnsiColor::Cyan,
        "gray" => iyon_tui::AnsiColor::Gray,
        "darkgray" => iyon_tui::AnsiColor::DarkGray,
        "lightred" => iyon_tui::AnsiColor::LightRed,
        "lightgreen" => iyon_tui::AnsiColor::LightGreen,
        "lightyellow" => iyon_tui::AnsiColor::LightYellow,
        "lightblue" => iyon_tui::AnsiColor::LightBlue,
        "lightmagenta" => iyon_tui::AnsiColor::LightMagenta,
        "lightcyan" => iyon_tui::AnsiColor::LightCyan,
        "white" => iyon_tui::AnsiColor::White,
        _ => return None,
    };
    Some(iyon_tui::ColorSpec::named(color))
}

fn text_attribute(value: &str) -> Option<iyon_tui::TextAttribute> {
    match value {
        "bold" => Some(iyon_tui::TextAttribute::Bold),
        "dim" => Some(iyon_tui::TextAttribute::Dim),
        "italic" => Some(iyon_tui::TextAttribute::Italic),
        "underline" => Some(iyon_tui::TextAttribute::Underline),
        "reversed" => Some(iyon_tui::TextAttribute::Reversed),
        "strikethrough" => Some(iyon_tui::TextAttribute::Strikethrough),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn lowers_nested_composition_through_canonical_views() {
        let value = json!({
            "type": "column",
            "children": [
                {"type": "text", "spans": [{"text": "one"}]},
                {"type": "row", "children": [{"type": "text", "spans": [{"text": "two"}]}]}
            ]
        });
        assert!(lower_view(&value).is_ok());
    }

    #[test]
    fn rejects_unknown_nodes_before_native_construction() {
        let error = lower_view(&json!({"type": "unknown"})).unwrap_err();
        assert!(error.to_string().contains("unknown view node type"));
    }

    #[test]
    fn native_text_input_owns_unicode_cursor_state() {
        let input = NativeTextInput::new(None);
        input.set_text("hello 🌍".into()).unwrap();
        assert_eq!(input.text().unwrap(), "hello 🌍");
        assert_eq!(input.cursor_bytes().unwrap(), "hello 🌍".len() as i64);
        input.dispose();
        assert!(input.text().is_err());
    }

    #[test]
    fn native_stream_rejects_updates_after_seal() {
        let stream = NativeTextStream::new();
        stream.update("first".into()).unwrap();
        stream.seal().unwrap();
        assert!(stream.update("late".into()).is_err());
    }
}

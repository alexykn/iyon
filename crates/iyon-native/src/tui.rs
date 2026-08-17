use napi::bindgen_prelude::{Reference, Result};
use napi_derive::napi;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use iyon_tui::projection::ProjectionBuilder;
use iyon_tui::stream::{StreamOffset, StreamRange};
use iyon_tui::text::{FormatId, LanguageId, SemanticTag, TextOrigin};
use iyon_tui::text::{TextRun, TextVisitor};
use iyon_tui::{
    BorderEdges, BorderGlyphs, BorderSpec, Component, GridCellSpec, GridTrack, History,
    HorizontalAlign, HostActivityConfig, HostCellStyle, HostHistory, HostScrollPane,
    HostStreamSegmentKind, HostTextInput, HostTextStream, HostViewSlot, HostWorking, IntoView, Key,
    KeyStroke, MarkdownOptions, MarkdownProjector, Modifiers, Output, Projector, StyleRef,
    StyleSpec, TextContent, TextInput, TextPart, TextRole, TextSelector, TextSpan,
    TextStreamPresentation, TuiHost, VerticalAlign, View, WrapMode,
};
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

    #[napi(js_name = "isDetached")]
    pub fn is_detached(&self) -> bool {
        self.host.is_none()
    }

    fn take_for_host(&mut self) -> Result<History> {
        if self.host.is_some() {
            return Err(crate::NativeError::invalid_input(
                "history is already attached to a native host",
            ));
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| crate::NativeError::internal("history lock is poisoned"))?;
        Ok(std::mem::replace(&mut *state, History::new()))
    }

    #[napi]
    pub fn layout(&self) -> Result<Value> {
        ensure_alive(&self.alive)?;
        if let Some(host) = &self.host {
            let layout = host
                .layout()
                .map_err(|error| crate::NativeError::internal(error.to_string()))?;
            return Ok(
                serde_json::json!({"padding": layout.padding().bottom(), "gap": layout.gap()}),
            );
        }
        let _layout = self
            .state
            .lock()
            .map_err(|_| crate::NativeError::internal("history lock is poisoned"))?
            .layout();
        Ok(serde_json::json!({"padding": _layout.padding().bottom(), "gap": _layout.gap()}))
    }

    #[napi(js_name = "setLayout")]
    pub fn set_layout(&self, value: Value) -> Result<()> {
        ensure_alive(&self.alive)?;
        let object = value
            .as_object()
            .ok_or_else(|| crate::NativeError::invalid_input("history layout must be an object"))?;
        let padding = u16_value(object, "padding")?;
        let gap = u16_value(object, "gap")?;
        let layout =
            iyon_tui::HistoryLayout::from_parts(iyon_tui::Insets::new(0, 0, padding, 0), gap);
        if let Some(host) = &self.host {
            return host
                .set_layout(layout)
                .map_err(|error| crate::NativeError::internal(error.to_string()));
        }
        self.state
            .lock()
            .map_err(|_| crate::NativeError::internal("history lock is poisoned"))?
            .set_layout(layout);
        Ok(())
    }

    #[napi]
    pub fn push(&self, view: &NativeTuiView) -> Result<i64> {
        ensure_alive(&self.alive)?;
        if let Some(host) = &self.host {
            return host
                .push(view.view.clone())
                .map(|unit| unit.value() as i64)
                .map_err(|error| crate::NativeError::invalid_input(error.to_string()));
        }
        self.state
            .lock()
            .map_err(|_| crate::NativeError::internal("history lock is poisoned"))?
            .push(view.view.clone())
            .map(|unit| unit.value() as i64)
            .map_err(|error| crate::NativeError::invalid_input(error.to_string()))
    }

    #[napi]
    pub fn freeze(&self, unit: i64, view: &NativeTuiView) -> Result<()> {
        ensure_alive(&self.alive)?;
        let unit = u64::try_from(unit)
            .map_err(|_| crate::NativeError::invalid_input("history unit id must be positive"))?;
        if let Some(host) = &self.host {
            return host
                .freeze(unit, view.view.clone())
                .map_err(|error| crate::NativeError::invalid_input(error.to_string()));
        }
        Err(crate::NativeError::invalid_input(
            "detached history cannot freeze a unit",
        ))
    }

    #[napi(js_name = "discardLive")]
    pub fn discard_live(&self, unit: i64) -> Result<()> {
        ensure_alive(&self.alive)?;
        let unit = u64::try_from(unit)
            .map_err(|_| crate::NativeError::invalid_input("history unit id must be positive"))?;
        if let Some(host) = &self.host {
            return host
                .discard_live(unit)
                .map_err(|error| crate::NativeError::invalid_input(error.to_string()));
        }
        Err(crate::NativeError::invalid_input(
            "detached history cannot discard a unit",
        ))
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

    #[napi(js_name = "sealStream")]
    pub fn seal_stream(&self, stream: &NativeTextStream) -> Result<()> {
        ensure_alive(&self.alive)?;
        if let Some(host) = &self.host {
            return host
                .seal_stream(&stream.stream)
                .map_err(|error| crate::NativeError::invalid_input(error.to_string()));
        }
        let mut history = self
            .state
            .lock()
            .map_err(|_| crate::NativeError::internal("history lock is poisoned"))?;
        stream
            .stream
            .seal_history(&mut history)
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
        let width = u16::try_from(width)
            .map_err(|_| crate::NativeError::invalid_input("width must fit in u16"))?;
        let height = u16::try_from(height)
            .map_err(|_| crate::NativeError::invalid_input("height must fit in u16"))?;
        let host = TuiHost::open(width, height, headless.unwrap_or(false))
            .map_err(|error| crate::NativeError::internal(error.to_string()))?;
        Ok(Self {
            host,
            alive: AtomicBool::new(true),
        })
    }

    #[napi]
    pub fn dispose(&self) -> Result<()> {
        if self.alive.swap(false, Ordering::AcqRel) {
            self.host
                .close()
                .map_err(|error| crate::NativeError::internal(error.to_string()))?;
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

    #[napi(js_name = "nextWakeMs")]
    pub fn next_wake_ms(&self) -> Result<i64> {
        ensure_alive(&self.alive)?;
        Ok(i64::try_from(self.host.next_wake_ms()).unwrap_or(i64::MAX))
    }

    #[napi]
    pub fn set_theme(&self, value: Value) -> Result<()> {
        ensure_alive(&self.alive)?;
        self.host
            .set_theme(lower_theme(&value)?)
            .map_err(|error| crate::NativeError::internal(error.to_string()))
    }

    #[napi(js_name = "setHistory")]
    pub fn set_history(&self, history: &mut NativeHistory) -> Result<()> {
        ensure_alive(&self.alive)?;
        let detached = history.take_for_host()?;
        self.host
            .set_history(detached)
            .map_err(|error| crate::NativeError::internal(error.to_string()))?;
        history.host = Some(self.host.history());
        Ok(())
    }

    #[napi]
    pub fn exited(&self) -> Result<bool> {
        Ok(self.host.exited())
    }

    #[napi(js_name = "styleAt")]
    pub fn style_at(&self, row: i64, column: i64) -> Result<Option<Value>> {
        ensure_alive(&self.alive)?;
        let row = u16::try_from(row)
            .map_err(|_| crate::NativeError::invalid_input("row must fit in u16"))?;
        let column = u16::try_from(column)
            .map_err(|_| crate::NativeError::invalid_input("column must fit in u16"))?;
        Ok(self.host.style_at(row, column).map(cell_style_value))
    }

    #[napi(js_name = "cellXOfText")]
    pub fn cell_x_of_text(&self, row: i64, text: String) -> Result<Option<i64>> {
        ensure_alive(&self.alive)?;
        let row = u16::try_from(row)
            .map_err(|_| crate::NativeError::invalid_input("row must fit in u16"))?;
        Ok(self.host.cell_x_of_text(row, &text).map(i64::from))
    }

    #[napi]
    pub fn history(&self) -> Result<NativeHistory> {
        ensure_alive(&self.alive)?;
        Ok(NativeHistory::from_host(self.host.history()))
    }

    #[napi(js_name = "textInput")]
    pub fn text_input(
        &self,
        multiline: Option<bool>,
        border: Option<Value>,
    ) -> Result<NativeTextInput> {
        ensure_alive(&self.alive)?;
        let input = self
            .host
            .create_text_input(multiline.unwrap_or(false))
            .map_err(|error| crate::NativeError::internal(error.to_string()))?;
        if let Some(border) = border {
            input
                .set_border(lower_border(&border)?)
                .map_err(|error| crate::NativeError::internal(error.to_string()))?;
        }
        Ok(NativeTextInput::from_host(input))
    }

    #[napi(js_name = "working")]
    pub fn working(&self, config: Option<Value>) -> Result<NativeWorking> {
        ensure_alive(&self.alive)?;
        let working = self
            .host
            .create_working(activity_config(config.as_ref())?)
            .map_err(|error| crate::NativeError::internal(error.to_string()))?;
        Ok(NativeWorking {
            working,
            alive: AtomicBool::new(true),
        })
    }

    #[napi(js_name = "createViewSlot")]
    pub fn create_view_slot(&self, initial: &NativeTuiView) -> Result<NativeViewSlot> {
        ensure_alive(&self.alive)?;
        let slot = self
            .host
            .create_view_slot(initial.view.clone())
            .map_err(|error| crate::NativeError::internal(error.to_string()))?;
        Ok(NativeViewSlot::from_host(slot))
    }

    #[napi(js_name = "scrollPane")]
    pub fn scroll_pane(&self, initial: &NativeTuiView) -> Result<NativeScrollPane> {
        ensure_alive(&self.alive)?;
        let pane = self
            .host
            .create_scroll_pane(initial.view.clone())
            .map_err(|error| crate::NativeError::internal(error.to_string()))?;
        Ok(NativeScrollPane::from_host(pane))
    }

    #[napi(js_name = "bindKey")]
    pub fn bind_key(
        &self,
        key: String,
        modifiers: Option<Vec<String>>,
        action_id: String,
    ) -> Result<()> {
        ensure_alive(&self.alive)?;
        self.host
            .bind_key(parse_key(&key, modifiers.as_deref())?, action_id)
            .map_err(|error| crate::NativeError::invalid_input(error.to_string()))
    }

    #[napi]
    pub fn route(&self, output: &NativeTuiOutput, action_id: String) -> Result<()> {
        ensure_alive(&self.alive)?;
        self.host
            .route_text_input_output(output.output, action_id)
            .map_err(|error| crate::NativeError::invalid_input(error.to_string()))
    }

    #[napi(js_name = "interceptPaste")]
    pub fn intercept_paste(&self, input: &NativeTextInput, action_id: String) -> Result<()> {
        ensure_alive(&self.alive)?;
        let host_input = input
            .host
            .as_ref()
            .ok_or_else(|| crate::NativeError::invalid_input("text input is not mounted"))?;
        self.host
            .intercept_paste(host_input, action_id)
            .map_err(|error| crate::NativeError::invalid_input(error.to_string()))
    }

    #[napi]
    pub fn render(&self, view: &NativeTuiView) -> Result<()> {
        ensure_alive(&self.alive)?;
        self.host
            .render(view.view.clone())
            .map_err(|error| crate::NativeError::internal(error.to_string()))
    }

    #[napi(js_name = "dispatchKey")]
    pub fn dispatch_key(&self, key: String, modifiers: Option<Vec<String>>) -> Result<()> {
        ensure_alive(&self.alive)?;
        self.host
            .dispatch_key(parse_key(&key, modifiers.as_deref())?)
            .map_err(|error| crate::NativeError::internal(error.to_string()))
    }

    #[napi(js_name = "dispatchPaste")]
    pub fn dispatch_paste(&self, text: String) -> Result<()> {
        ensure_alive(&self.alive)?;
        self.host
            .dispatch_paste(&text)
            .map_err(|error| crate::NativeError::internal(error.to_string()))
    }

    #[napi(js_name = "forwardPaste")]
    pub fn forward_paste(&self, text: String) -> Result<()> {
        ensure_alive(&self.alive)?;
        self.host
            .forward_paste(&text)
            .map_err(|error| crate::NativeError::internal(error.to_string()))
    }

    #[napi(js_name = "pollTerminal")]
    pub fn poll_terminal(&self) -> Result<()> {
        ensure_alive(&self.alive)?;
        self.host
            .poll_terminal()
            .map_err(|error| crate::NativeError::internal(error.to_string()))
    }

    #[napi(js_name = "nextAction")]
    pub fn next_action(&self) -> Result<Option<Value>> {
        ensure_alive(&self.alive)?;
        Ok(self.host.next_action().map(
            |action| serde_json::json!({"action_id": action.action_id, "payload": action.payload}),
        ))
    }

    /// Wait in the native TUI driver until Rust has routed a semantic action
    /// or the host exits. Raw terminal events never cross this boundary.
    #[napi(js_name = "waitForAction")]
    pub async fn wait_for_action(&self) -> Result<Option<Value>> {
        ensure_alive(&self.alive)?;
        let host = self.host.clone();
        let action = host
            .wait_for_action()
            .await
            .map_err(|error| crate::NativeError::internal(error.to_string()))?;
        Ok(action.map(
            |action| serde_json::json!({"action_id": action.action_id, "payload": action.payload}),
        ))
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
        let width = u16::try_from(width)
            .map_err(|_| crate::NativeError::invalid_input("width must fit in u16"))?;
        let height = u16::try_from(height)
            .map_err(|_| crate::NativeError::invalid_input("height must fit in u16"))?;
        self.host
            .resize(width, height)
            .map_err(|error| crate::NativeError::internal(error.to_string()))
    }

    #[napi(js_name = "advanceTime")]
    pub fn advance_time(&self, milliseconds: i64) -> Result<()> {
        ensure_alive(&self.alive)?;
        let milliseconds = u64::try_from(milliseconds)
            .map_err(|_| crate::NativeError::invalid_input("time must be non-negative"))?;
        self.host
            .advance_time(std::time::Duration::from_millis(milliseconds))
            .map_err(|error| crate::NativeError::internal(error.to_string()))
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
            let Some(character) = chars.next() else {
                return Err(crate::NativeError::invalid_input("key must not be empty"));
            };
            if chars.next().is_some() {
                return Err(crate::NativeError::invalid_input(
                    "character key must contain one character",
                ));
            }
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
            other => {
                return Err(crate::NativeError::invalid_input(format!(
                    "unknown key modifier `{other}`"
                )));
            }
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
    pub fn new(projector: Option<String>) -> Self {
        Self {
            stream: if projector.as_deref() == Some("markdown") {
                HostTextStream::with_markdown_presentation(TextStreamPresentation::new(
                    iyon_tui::Insets::new(0, 2, 0, 2),
                ))
            } else {
                HostTextStream::new()
            },
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

    #[napi(js_name = "appendSegment")]
    pub fn append_segment(&self, kind: String, text: String) -> Result<()> {
        ensure_alive(&self.alive)?;
        let kind = match kind.as_str() {
            "text" => HostStreamSegmentKind::Text,
            "thinking" => HostStreamSegmentKind::Thinking,
            _ => {
                return Err(crate::NativeError::invalid_input(
                    "segment kind must be text or thinking",
                ));
            }
        };
        self.stream
            .append_segment(kind, text)
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
        let (text, revision, sealed, segments) = self
            .stream
            .snapshot_json()
            .map_err(|error| crate::NativeError::internal(error.to_string()))?;
        let mut snapshot =
            serde_json::json!({"text": text, "revision": revision, "sealed": sealed});
        if !segments.is_empty() {
            snapshot["segments"] = serde_json::Value::Array(
                segments
                    .into_iter()
                    .map(|(kind, text)| serde_json::json!({"kind": kind, "text": text}))
                    .collect(),
            );
        }
        Ok(snapshot)
    }
}

#[napi]
pub struct NativeMarkdownProjector {
    projector: Mutex<MarkdownProjector>,
    alive: AtomicBool,
}

#[napi]
pub struct NativePlainProjector {
    alive: AtomicBool,
}

#[napi]
impl NativePlainProjector {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            alive: AtomicBool::new(true),
        }
    }

    #[napi]
    pub fn dispose(&self) {
        self.alive.store(false, Ordering::Release);
    }

    #[napi]
    pub fn project(&self, text: String) -> Result<Value> {
        ensure_alive(&self.alive)?;
        let length = text.len() as u64;
        Ok(serde_json::json!({
            "spans": [{"sourceStart": 0, "sourceEnd": length, "text": text}],
        }))
    }
}

#[napi]
impl NativeMarkdownProjector {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            projector: Mutex::new(MarkdownProjector::new(MarkdownOptions::commonmark())),
            alive: AtomicBool::new(true),
        }
    }

    #[napi]
    pub fn dispose(&self) {
        self.alive.store(false, Ordering::Release);
    }

    #[napi]
    pub fn project(&self, text: String, sealed: Option<bool>) -> Result<Value> {
        ensure_alive(&self.alive)?;
        let sealed = sealed.unwrap_or(true);
        let end = StreamOffset::new(text.len() as u64);
        let input = ProjectionBuilder::new(
            StreamOffset::ZERO,
            if sealed { end } else { StreamOffset::ZERO },
            end,
            sealed,
        )
        .emit(
            StreamRange::new(StreamOffset::ZERO, end),
            TextContent::raw(text),
        )
        .finish()
        .map_err(|error| crate::NativeError::invalid_input(error.to_string()))?;
        let projection = self
            .projector
            .lock()
            .map_err(|_| crate::NativeError::internal("markdown projector lock is poisoned"))?
            .project(&input)
            .map_err(|error| crate::NativeError::invalid_input(error.to_string()))?;
        let spans = projection
            .spans()
            .iter()
            .map(|span| {
                let mut output = String::new();
                for value in span.values() {
                    let mut visitor = PlainTextVisitor {
                        output: String::new(),
                    };
                    visitor.visit_content(value);
                    output.push_str(&visitor.output);
                }
                serde_json::json!({
                    "sourceStart": span.source().start().as_u64(),
                    "sourceEnd": span.source().end().as_u64(),
                    "text": output,
                })
            })
            .collect::<Vec<_>>();
        Ok(serde_json::json!({"spans": spans}))
    }
}

struct PlainTextVisitor {
    output: String,
}

impl TextVisitor for PlainTextVisitor {
    fn visit_raw(&mut self, raw: &iyon_tui::RawText) {
        self.output.push_str(raw.text());
    }

    fn visit_text_run(&mut self, run: &TextRun) {
        self.output.push_str(run.text());
    }
}

#[napi]
pub struct NativeViewSlot {
    slot: HostViewSlot,
    alive: AtomicBool,
}

#[napi]
pub struct NativeScrollPane {
    pane: HostScrollPane,
    alive: AtomicBool,
}

#[napi]
impl NativeScrollPane {
    #[napi(constructor)]
    pub fn new(initial: &NativeTuiView) -> Self {
        Self {
            pane: HostScrollPane::new(initial.view.clone()),
            alive: AtomicBool::new(true),
        }
    }

    #[napi]
    pub fn dispose(&self) {
        self.alive.store(false, Ordering::Release);
    }

    #[napi(js_name = "componentId")]
    pub fn component_id(&self) -> Result<Option<i64>> {
        ensure_alive(&self.alive)?;
        Ok(self.pane.component_id().map(|id| id as i64))
    }

    #[napi(js_name = "setContent")]
    pub fn set_content(&self, view: &NativeTuiView) -> Result<()> {
        ensure_alive(&self.alive)?;
        self.pane
            .set_content(view.view.clone())
            .map_err(|error| crate::NativeError::internal(error.to_string()))
    }

    #[napi(js_name = "followEnd")]
    pub fn follow_end(&self) -> Result<()> {
        ensure_alive(&self.alive)?;
        self.pane
            .follow_end()
            .map_err(|error| crate::NativeError::internal(error.to_string()))
    }

    fn from_host(pane: HostScrollPane) -> Self {
        Self {
            pane,
            alive: AtomicBool::new(true),
        }
    }
}

#[napi]
impl NativeViewSlot {
    #[napi(constructor)]
    pub fn new(initial: &NativeTuiView) -> Self {
        Self {
            slot: HostViewSlot::new(initial.view.clone()),
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
        Ok(self.slot.revision() as i64)
    }

    #[napi(js_name = "componentId")]
    pub fn component_id(&self) -> Result<Option<i64>> {
        ensure_alive(&self.alive)?;
        Ok(self.slot.component_id().map(|id| id as i64))
    }

    #[napi(js_name = "setView")]
    pub fn set_view(&self, view: &NativeTuiView) -> Result<()> {
        ensure_alive(&self.alive)?;
        self.slot
            .set_view(view.view.clone())
            .map_err(|error| crate::NativeError::internal(error.to_string()))
    }

    #[napi(js_name = "setAnimation")]
    pub fn set_animation(
        &self,
        frames: Vec<Reference<NativeTuiView>>,
        interval_ms: i64,
    ) -> Result<()> {
        ensure_alive(&self.alive)?;
        let interval_ms = u64::try_from(interval_ms).map_err(|_| {
            crate::NativeError::invalid_input("animation interval must be positive")
        })?;
        if interval_ms == 0 {
            return Err(crate::NativeError::invalid_input(
                "animation interval must be positive",
            ));
        }
        if frames.is_empty() {
            return Err(crate::NativeError::invalid_input(
                "animation requires at least one frame",
            ));
        }
        self.slot
            .set_animation(
                frames.into_iter().map(|frame| frame.view.clone()).collect(),
                std::time::Duration::from_millis(interval_ms),
            )
            .map_err(|error| crate::NativeError::internal(error.to_string()))
    }

    #[napi(js_name = "stopAnimation")]
    pub fn stop_animation(&self, view: &NativeTuiView) -> Result<()> {
        ensure_alive(&self.alive)?;
        self.slot
            .stop_animation(view.view.clone())
            .map_err(|error| crate::NativeError::internal(error.to_string()))
    }

    fn from_host(slot: HostViewSlot) -> Self {
        Self {
            slot,
            alive: AtomicBool::new(true),
        }
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
            let spans = spans
                .iter()
                .map(lower_text_span)
                .collect::<Result<Vec<_>>>()?;
            let text = View::styled_text(spans);
            let text = match object
                .get("wrap")
                .and_then(Value::as_str)
                .unwrap_or("wordThenGrapheme")
            {
                "wordThenGrapheme" => text.wrap(WrapMode::WordThenGrapheme),
                "grapheme" => text.wrap(WrapMode::Grapheme),
                "noWrap" => text.wrap(WrapMode::NoWrap),
                other => {
                    return Err(crate::NativeError::invalid_input(format!(
                        "unknown wrap mode `{other}`"
                    )));
                }
            };
            let text = match object
                .get("align")
                .and_then(Value::as_str)
                .unwrap_or("start")
            {
                "start" => text.text_align(HorizontalAlign::Start),
                "center" => text.text_align(HorizontalAlign::Center),
                "end" => text.text_align(HorizontalAlign::End),
                other => {
                    return Err(crate::NativeError::invalid_input(format!(
                        "unknown text alignment `{other}`"
                    )));
                }
            };
            text.into_view()
        }
        "spacer" => {
            let rows = u16_value(object, "rows")?;
            View::spacer(rows)
        }
        "row" => lower_axis(object, true)?,
        "column" => lower_axis(object, false)?,
        "hanging" => View::hanging(
            lower_required(object, "prefix")?,
            lower_required(object, "continuation")?,
            lower_required(object, "body")?,
        ),
        "grid" => lower_grid(object)?,
        "container" => lower_required(object, "child")?.container(),
        "clamp" => lower_required(object, "child")?.clamp_rows(
            u16_value(object, "maxRows")?,
            lower_overflow(object.get("overflow"))?,
        ),
        "decorated" => {
            apply_decoration(lower_required(object, "child")?, object.get("decoration"))?
        }
        "component" => {
            View::native_component(object.get("handle").and_then(Value::as_u64).ok_or_else(
                || crate::NativeError::invalid_input("component handle must be an integer"),
            )?)
        }
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

fn lower_axis(object: &Map<String, Value>, horizontal: bool) -> Result<View> {
    let gap = u16_value(object, "gap")?;
    let children = object
        .get("children")
        .and_then(Value::as_array)
        .ok_or_else(|| crate::NativeError::invalid_input("view children must be an array"))?;
    let mut lowered = Vec::with_capacity(children.len());
    for child in children {
        let child = child
            .as_object()
            .ok_or_else(|| crate::NativeError::invalid_input("layout child must be an object"))?;
        let kind = child.get("kind").and_then(Value::as_str).ok_or_else(|| {
            crate::NativeError::invalid_input("layout child kind must be a string")
        })?;
        let view =
            lower_view(child.get("child").ok_or_else(|| {
                crate::NativeError::invalid_input("layout child view is required")
            })?)?;
        let size = child
            .get("size")
            .map(|_| u16_value(child, "size"))
            .transpose()?;
        let max_rows = child
            .get("maxRows")
            .map(|_| u16_value(child, "maxRows"))
            .transpose()?;
        match kind {
            "normal" | "flex" => {}
            "fixed" if size.is_some() => {}
            "fixed" => {
                return Err(crate::NativeError::invalid_input(
                    "fixed layout child size is required",
                ));
            }
            "contentMax" if !horizontal && max_rows.is_some() => {}
            "contentMax" if !horizontal => {
                return Err(crate::NativeError::invalid_input(
                    "contentMax maxRows is required",
                ));
            }
            "contentMax" => {
                return Err(crate::NativeError::invalid_input(
                    "contentMax is only valid for vertical children",
                ));
            }
            "flexMax" if !horizontal && max_rows.is_some() => {}
            "flexMax" if !horizontal => {
                return Err(crate::NativeError::invalid_input(
                    "flexMax maxRows is required",
                ));
            }
            "flexMax" => {
                return Err(crate::NativeError::invalid_input(
                    "flexMax is only valid for vertical children",
                ));
            }
            other => {
                return Err(crate::NativeError::invalid_input(format!(
                    "unknown layout child kind `{other}`"
                )));
            }
        }
        lowered.push((kind.to_owned(), size, max_rows, view));
    }
    if horizontal {
        Ok(View::horizontal(|row| {
            row.gap(gap);
            for (kind, size, _max_rows, view) in lowered {
                match kind.as_str() {
                    "normal" => {
                        row.child(view);
                    }
                    "fixed" => {
                        row.fixed(size.expect("fixed size was validated"), view);
                    }
                    "flex" => {
                        row.flex(view);
                    }
                    "contentMax" => unreachable!("contentMax was rejected for horizontal layout"),
                    "flexMax" => unreachable!("flexMax was rejected for horizontal layout"),
                    _ => unreachable!("layout child kind was validated"),
                }
            }
        }))
    } else {
        Ok(View::vertical(|column| {
            column.gap(gap);
            for (kind, size, max_rows, view) in lowered {
                match kind.as_str() {
                    "normal" => {
                        column.child(view);
                    }
                    "fixed" => {
                        column.fixed(size.expect("fixed size was validated"), view);
                    }
                    "flex" => {
                        column.flex(view);
                    }
                    "contentMax" => {
                        column.content_max(max_rows.expect("validated content max"), view);
                    }
                    "flexMax" => {
                        column.flex_max(max_rows.expect("validated flex max"), view);
                    }
                    _ => unreachable!("layout child kind was validated"),
                }
            }
        }))
    }
}

fn lower_grid(object: &Map<String, Value>) -> Result<View> {
    let columns = object
        .get("columns")
        .and_then(Value::as_array)
        .ok_or_else(|| crate::NativeError::invalid_input("grid columns must be an array"))?
        .iter()
        .map(lower_grid_track)
        .collect::<Result<Vec<_>>>()?;
    let rows = object
        .get("rows")
        .and_then(Value::as_array)
        .ok_or_else(|| crate::NativeError::invalid_input("grid rows must be an array"))?;
    let column_gap = u16_value(object, "columnGap")?;
    let row_gap = u16_value(object, "rowGap")?;
    let mut lowered_rows = Vec::with_capacity(rows.len());
    for row in rows {
        let row = row
            .as_object()
            .ok_or_else(|| crate::NativeError::invalid_input("grid row must be an object"))?;
        let track = lower_grid_track(
            row.get("track")
                .ok_or_else(|| crate::NativeError::invalid_input("grid row track is required"))?,
        )?;
        let cells = row
            .get("cells")
            .and_then(Value::as_array)
            .ok_or_else(|| crate::NativeError::invalid_input("grid cells must be an array"))?;
        let mut lowered_cells = Vec::with_capacity(cells.len());
        for cell in cells {
            let cell = cell
                .as_object()
                .ok_or_else(|| crate::NativeError::invalid_input("grid cell must be an object"))?;
            let spec = GridCellSpec::new()
                .column_span(u16_value(cell, "columnSpan")?)
                .row_span(u16_value(cell, "rowSpan")?)
                .horizontal_align(parse_horizontal_align(
                    cell.get("horizontalAlign")
                        .and_then(Value::as_str)
                        .unwrap_or("start"),
                )?)
                .vertical_align(parse_vertical_align(
                    cell.get("verticalAlign")
                        .and_then(Value::as_str)
                        .unwrap_or("top"),
                )?);
            lowered_cells.push((spec, lower_required(cell, "view")?));
        }
        lowered_rows.push((track, lowered_cells));
    }
    Ok(View::grid(|grid| {
        grid.columns(columns);
        grid.column_gap(column_gap);
        grid.row_gap(row_gap);
        for (track, cells) in lowered_rows {
            grid.row_with(track, |row| {
                for (spec, view) in &cells {
                    row.cell_with(*spec, view.clone());
                }
            });
        }
    }))
}

fn lower_grid_track(value: &Value) -> Result<GridTrack> {
    let object = value
        .as_object()
        .ok_or_else(|| crate::NativeError::invalid_input("grid track must be an object"))?;
    match object
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| crate::NativeError::invalid_input("grid track kind must be a string"))?
    {
        "content" => Ok(GridTrack::content()),
        "contentMax" => Ok(GridTrack::content_max(u16_value(object, "max")?)),
        "fixed" => Ok(GridTrack::fixed(u16_value(object, "size")?)),
        "flex" => Ok(GridTrack::flex()),
        "flexMax" => Ok(GridTrack::flex_max(u16_value(object, "max")?)),
        other => Err(crate::NativeError::invalid_input(format!(
            "unknown grid track kind `{other}`"
        ))),
    }
}

fn lower_overflow(value: Option<&Value>) -> Result<iyon_tui::OverflowIndicator> {
    let Some(value) = value else {
        return Ok(iyon_tui::OverflowIndicator::None);
    };
    let object = value
        .as_object()
        .ok_or_else(|| crate::NativeError::invalid_input("overflow indicator must be an object"))?;
    let kind = object.get("kind").and_then(Value::as_str).ok_or_else(|| {
        crate::NativeError::invalid_input("overflow indicator kind must be a string")
    })?;
    match kind {
        "none" => Ok(iyon_tui::OverflowIndicator::None),
        "ellipsis" => {
            Ok(iyon_tui::OverflowIndicator::Ellipsis {
                style: lower_style_ref(object.get("style").ok_or_else(|| {
                    crate::NativeError::invalid_input("ellipsis style is required")
                })?)?,
            })
        }
        "footer" => Ok(iyon_tui::OverflowIndicator::Footer {
            prefix: object
                .get("prefix")
                .and_then(Value::as_str)
                .ok_or_else(|| crate::NativeError::invalid_input("footer prefix must be a string"))?
                .to_owned(),
            style: lower_style_ref(
                object
                    .get("style")
                    .ok_or_else(|| crate::NativeError::invalid_input("footer style is required"))?,
            )?,
        }),
        other => Err(crate::NativeError::invalid_input(format!(
            "unknown overflow indicator `{other}`"
        ))),
    }
}

fn parse_horizontal_align(value: &str) -> Result<HorizontalAlign> {
    match value {
        "start" => Ok(HorizontalAlign::Start),
        "center" => Ok(HorizontalAlign::Center),
        "end" => Ok(HorizontalAlign::End),
        other => Err(crate::NativeError::invalid_input(format!(
            "unknown horizontal alignment `{other}`"
        ))),
    }
}

fn parse_vertical_align(value: &str) -> Result<VerticalAlign> {
    match value {
        "top" => Ok(VerticalAlign::Top),
        "center" => Ok(VerticalAlign::Center),
        "bottom" => Ok(VerticalAlign::Bottom),
        other => Err(crate::NativeError::invalid_input(format!(
            "unknown vertical alignment `{other}`"
        ))),
    }
}

fn lower_text_span(value: &Value) -> Result<TextSpan> {
    let object = value
        .as_object()
        .ok_or_else(|| crate::NativeError::invalid_input("text span must be an object"))?;
    let text = object
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| crate::NativeError::invalid_input("text span text must be a string"))?;
    let style = object
        .get("style")
        .map(lower_style_ref)
        .transpose()?
        .unwrap_or_else(|| StyleRef::direct(StyleSpec::new()));
    Ok(TextSpan::styled(text, style))
}

fn lower_style_ref(value: &Value) -> Result<StyleRef> {
    let object = value
        .as_object()
        .ok_or_else(|| crate::NativeError::invalid_input("style must be an object"))?;
    let style = lower_style_spec(value)?;
    Ok(match object.get("theme").and_then(Value::as_str) {
        Some(theme) => StyleRef::themed(theme, style),
        None => StyleRef::direct(style),
    })
}

fn lower_style_spec(value: &Value) -> Result<StyleSpec> {
    let object = value
        .as_object()
        .ok_or_else(|| crate::NativeError::invalid_input("style must be an object"))?;
    let mut style = StyleSpec::new();
    if let Some(color) = object.get("foreground") {
        style = style.foreground(color_spec(color)?);
    }
    if let Some(color) = object.get("background") {
        style = style.background(color_spec(color)?);
    }
    if let Some(attributes) = object.get("attributes").and_then(Value::as_object) {
        for (name, enabled) in attributes {
            let attribute = text_attribute(name).ok_or_else(|| {
                crate::NativeError::invalid_input(format!("unknown text attribute `{name}`"))
            })?;
            let enabled = enabled.as_bool().ok_or_else(|| {
                crate::NativeError::invalid_input("text attributes must be booleans")
            })?;
            style = style.attribute(attribute, enabled);
        }
    }
    Ok(style)
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

fn activity_config(value: Option<&Value>) -> Result<HostActivityConfig> {
    let Some(value) = value else {
        return Ok(HostActivityConfig::default());
    };
    let object = value
        .as_object()
        .ok_or_else(|| crate::NativeError::invalid_input("activity config must be an object"))?;
    let mut config = HostActivityConfig::default();
    if let Some(frames) = object.get("frames") {
        config.frames = frames
            .as_array()
            .ok_or_else(|| crate::NativeError::invalid_input("activity frames must be an array"))?
            .iter()
            .map(|frame| {
                frame.as_str().map(str::to_owned).ok_or_else(|| {
                    crate::NativeError::invalid_input("activity frames must be strings")
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if config.frames.is_empty() {
            return Err(crate::NativeError::invalid_input(
                "activity frames must not be empty",
            ));
        }
    }
    if let Some(label) = object.get("activeLabel") {
        config.active_label = label
            .as_str()
            .ok_or_else(|| crate::NativeError::invalid_input("activeLabel must be a string"))?
            .to_owned();
    }
    if let Some(label) = object.get("pendingLabel") {
        config.pending_label = label
            .as_str()
            .ok_or_else(|| crate::NativeError::invalid_input("pendingLabel must be a string"))?
            .to_owned();
    }
    if let Some(prefix) = object.get("queuePrefix") {
        config.queue_prefix = prefix
            .as_str()
            .ok_or_else(|| crate::NativeError::invalid_input("queuePrefix must be a string"))?
            .to_owned();
    }
    if let Some(tick) = object.get("tickMs") {
        config.tick_ms = tick
            .as_u64()
            .ok_or_else(|| crate::NativeError::invalid_input("tickMs must be an integer"))?;
        if config.tick_ms == 0 {
            return Err(crate::NativeError::invalid_input("tickMs must be positive"));
        }
    }
    if let Some(padding) = object.get("padding") {
        config.padding = u16::try_from(padding.as_u64().ok_or_else(|| {
            crate::NativeError::invalid_input("activity padding must be an integer")
        })?)
        .map_err(|_| crate::NativeError::invalid_input("activity padding must fit in u16"))?;
    }
    if let Some(style) = object.get("mutedStyle") {
        config.muted_style = lower_style_ref(style)?;
    }
    Ok(config)
}

fn cell_style_value(style: HostCellStyle) -> Value {
    serde_json::json!({
        "foreground": style.foreground,
        "background": style.background,
        "bold": style.bold,
        "dim": style.dim,
        "italic": style.italic,
        "underline": style.underline,
        "reversed": style.reversed,
        "strikethrough": style.strikethrough,
    })
}

fn lower_theme(value: &Value) -> Result<iyon_tui::Theme> {
    let object = value
        .as_object()
        .ok_or_else(|| crate::NativeError::invalid_input("theme must be an object"))?;
    let mut theme = iyon_tui::Theme::new();
    if let Some(colors) = object.get("colors").and_then(Value::as_object) {
        for (key, entry) in colors {
            let entry = entry.as_object().ok_or_else(|| {
                crate::NativeError::invalid_input("theme color entry must be an object")
            })?;
            if let Some(base) = entry.get("base") {
                theme.set_color(key.as_str(), lower_theme_color(base)?);
            }
            for variant in entry
                .get("variants")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let variant = variant.as_object().ok_or_else(|| {
                    crate::NativeError::invalid_input("theme color variant must be an object")
                })?;
                theme.set_color_variant(
                    key.as_str(),
                    lower_selector(variant.get("selector").ok_or_else(|| {
                        crate::NativeError::invalid_input("theme color selector is required")
                    })?)?,
                    lower_theme_color(variant.get("value").ok_or_else(|| {
                        crate::NativeError::invalid_input("theme color value is required")
                    })?)?,
                );
            }
        }
    }
    if let Some(styles) = object.get("styles").and_then(Value::as_object) {
        for (key, entry) in styles {
            let entry = entry.as_object().ok_or_else(|| {
                crate::NativeError::invalid_input("theme style entry must be an object")
            })?;
            if let Some(base) = entry.get("base") {
                theme.set_style(key.as_str(), lower_style_spec(base)?);
            }
            for variant in entry
                .get("variants")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let variant = variant.as_object().ok_or_else(|| {
                    crate::NativeError::invalid_input("theme style variant must be an object")
                })?;
                theme.set_style_variant(
                    key.as_str(),
                    lower_selector(variant.get("selector").ok_or_else(|| {
                        crate::NativeError::invalid_input("theme style selector is required")
                    })?)?,
                    lower_style_spec(variant.get("value").ok_or_else(|| {
                        crate::NativeError::invalid_input("theme style value is required")
                    })?)?,
                );
            }
        }
    }
    if let Some(text_styles) = object.get("textStyles").and_then(Value::as_array) {
        for entry in text_styles {
            let entry = entry.as_object().ok_or_else(|| {
                crate::NativeError::invalid_input("theme text style entry must be an object")
            })?;
            let selector = lower_text_selector(entry.get("selector").ok_or_else(|| {
                crate::NativeError::invalid_input("theme text style selector is required")
            })?)?;
            let style = lower_style_spec(entry.get("value").ok_or_else(|| {
                crate::NativeError::invalid_input("theme text style value is required")
            })?)?;
            theme.set_text_style(selector, style);
        }
    }
    Ok(theme)
}

fn lower_text_selector(value: &Value) -> Result<TextSelector> {
    let object = value
        .as_object()
        .ok_or_else(|| crate::NativeError::invalid_input("text selector must be an object"))?;
    let mut selector = TextSelector::any();
    if let Some(roles) = object.get("roles").and_then(Value::as_array) {
        for role in roles {
            selector = selector.and_role(lower_text_role(role.as_str().ok_or_else(|| {
                crate::NativeError::invalid_input("text selector role must be a string")
            })?)?);
        }
    }
    if let Some(parts) = object.get("parts").and_then(Value::as_array) {
        for part in parts {
            selector = selector.and_part(lower_text_part(part.as_str().ok_or_else(|| {
                crate::NativeError::invalid_input("text selector part must be a string")
            })?)?);
        }
    }
    if let Some(annotations) = object.get("annotations").and_then(Value::as_array) {
        for annotation in annotations {
            let annotation = annotation.as_object().ok_or_else(|| {
                crate::NativeError::invalid_input("text selector annotation must be an object")
            })?;
            let namespace = annotation
                .get("namespace")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    crate::NativeError::invalid_input(
                        "text selector annotation namespace is required",
                    )
                })?;
            let name = annotation
                .get("name")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    crate::NativeError::invalid_input("text selector annotation name is required")
                })?;
            let tag = SemanticTag::new(namespace, name)
                .map_err(|error| crate::NativeError::invalid_input(error.to_string()))?;
            selector = selector.and_annotation(&tag);
        }
    }
    if let Some(language) = object.get("language").and_then(Value::as_str) {
        let language = LanguageId::new(language)
            .map_err(|error| crate::NativeError::invalid_input(error.to_string()))?;
        selector = selector.language(&language);
    }
    if let Some(origin) = object.get("origin").and_then(Value::as_str) {
        let origin = TextOrigin::new(origin)
            .map_err(|error| crate::NativeError::invalid_input(error.to_string()))?;
        selector = selector.origin(origin);
    }
    if let Some(format) = object.get("format").and_then(Value::as_str) {
        let format = FormatId::new(format)
            .map_err(|error| crate::NativeError::invalid_input(error.to_string()))?;
        selector = selector.format(&format);
    }
    if object
        .get("focused")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        selector = selector.and_focused();
    }
    if object
        .get("focusWithin")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        selector = selector.and_focus_within();
    }
    if let Some(states) = object.get("states").and_then(Value::as_object) {
        for (key, value) in states {
            selector = selector.and_state(
                key.clone(),
                value.as_str().ok_or_else(|| {
                    crate::NativeError::invalid_input("text selector states must be strings")
                })?,
            );
        }
    }
    Ok(selector)
}

fn lower_text_role(value: &str) -> Result<TextRole> {
    let role = match value {
        "paragraph" => TextRole::Paragraph,
        "heading" => TextRole::Heading,
        "blockQuote" => TextRole::BlockQuote,
        "list" => TextRole::List,
        "listItem" => TextRole::ListItem,
        "codeBlock" => TextRole::CodeBlock,
        "table" => TextRole::Table,
        "tableRow" => TextRole::TableRow,
        "tableCell" => TextRole::TableCell,
        "thematicBreak" => TextRole::ThematicBreak,
        "rawBlock" => TextRole::RawBlock,
        "container" => TextRole::Container,
        "strong" => TextRole::Strong,
        "emphasis" => TextRole::Emphasis,
        "strikethrough" => TextRole::Strikethrough,
        "underline" => TextRole::Underline,
        "superscript" => TextRole::Superscript,
        "subscript" => TextRole::Subscript,
        "smallCaps" => TextRole::SmallCaps,
        "inlineCode" => TextRole::InlineCode,
        "link" => TextRole::Link,
        "image" => TextRole::Image,
        "rawInline" => TextRole::RawInline,
        _ => {
            return Err(crate::NativeError::invalid_input(format!(
                "unknown text selector role `{value}`"
            )));
        }
    };
    Ok(role)
}

fn lower_text_part(value: &str) -> Result<TextPart> {
    let part = match value {
        "listMarker" => TextPart::ListMarker,
        "taskMarker" => TextPart::TaskMarker,
        "quoteMarker" => TextPart::QuoteMarker,
        "codeLabel" => TextPart::CodeLabel,
        "tableRule" => TextPart::TableRule,
        "thematicRule" => TextPart::ThematicRule,
        "imageFallback" => TextPart::ImageFallback,
        _ => {
            return Err(crate::NativeError::invalid_input(format!(
                "unknown text selector part `{value}`"
            )));
        }
    };
    Ok(part)
}

fn lower_selector(value: &Value) -> Result<iyon_tui::StyleSelector> {
    let object = value
        .as_object()
        .ok_or_else(|| crate::NativeError::invalid_input("theme selector must be an object"))?;
    let mut selector = iyon_tui::StyleSelector::default();
    if object
        .get("focused")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        selector = selector.and_focused();
    }
    if object
        .get("focusWithin")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        selector = selector.and_focus_within();
    }
    if let Some(states) = object.get("states").and_then(Value::as_object) {
        for (key, value) in states {
            selector = selector.and_state(
                key.clone(),
                value.as_str().ok_or_else(|| {
                    crate::NativeError::invalid_input("theme selector states must be strings")
                })?,
            );
        }
    }
    Ok(selector)
}

fn lower_theme_color(value: &Value) -> Result<iyon_tui::ThemeColor> {
    match color_spec(value)? {
        iyon_tui::ColorSpec::Theme(_) => Err(crate::NativeError::invalid_input(
            "theme colors cannot reference another theme color",
        )),
        iyon_tui::ColorSpec::Named(color) => Ok(iyon_tui::ThemeColor::Named(color)),
        iyon_tui::ColorSpec::Ansi(value) => Ok(iyon_tui::ThemeColor::Indexed(value)),
        iyon_tui::ColorSpec::Rgb { r, g, b } => Ok(iyon_tui::ThemeColor::Rgb { r, g, b }),
    }
}

fn lower_border(value: &Value) -> Result<BorderSpec> {
    let border = value
        .as_object()
        .ok_or_else(|| crate::NativeError::invalid_input("border must be an object"))?;
    let mut spec = match border
        .get("style")
        .and_then(Value::as_str)
        .unwrap_or("plain")
    {
        "plain" => BorderSpec::plain(),
        "rounded" => BorderSpec::rounded(),
        "double" => BorderSpec::double(),
        other => {
            return Err(crate::NativeError::invalid_input(format!(
                "unknown border style `{other}`"
            )));
        }
    };
    if border.get("edges").and_then(Value::as_str) == Some("topBottom") {
        spec = spec.edges(BorderEdges::TOP_BOTTOM);
    }
    if let Some(color) = border.get("color") {
        spec = spec.color(color_spec(color)?);
    }
    if let Some(glyphs) = border.get("glyphs").and_then(Value::as_object) {
        let fields = [
            "top",
            "right",
            "bottom",
            "left",
            "topLeft",
            "topRight",
            "bottomLeft",
            "bottomRight",
        ];
        let values = fields
            .iter()
            .map(|field| {
                glyphs.get(*field).and_then(Value::as_str).ok_or_else(|| {
                    crate::NativeError::invalid_input(format!(
                        "border glyph `{field}` must be a string"
                    ))
                })
            })
            .collect::<Result<Vec<_>>>()?;
        spec = BorderSpec::custom(
            BorderGlyphs::new(
                values[0], values[1], values[2], values[3], values[4], values[5], values[6],
                values[7],
            )
            .map_err(|error| crate::NativeError::invalid_input(error.to_string()))?,
        );
        if border.get("edges").and_then(Value::as_str) == Some("topBottom") {
            spec = spec.edges(BorderEdges::TOP_BOTTOM);
        }
        if let Some(color) = border.get("color") {
            spec = spec.color(color_spec(color)?);
        }
    }
    Ok(spec)
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
    if let Some(color) = decoration.get("background") {
        view = view.background(color_spec(color)?);
    }
    if let Some(color) = decoration.get("foreground") {
        view = view.foreground(color_spec(color)?);
    }
    if let Some(border) = decoration.get("border").and_then(Value::as_object) {
        view = view.border(lower_border(&Value::Object(border.clone()))?);
    }
    if let Some(style) = decoration.get("style").and_then(Value::as_object) {
        view = view.style(lower_style_ref(&Value::Object(style.clone()))?);
        if let Some(attributes) = style.get("attributes").and_then(Value::as_object) {
            for (name, enabled) in attributes {
                if let Some(attribute) = text_attribute(name) {
                    view = view.text_attribute(
                        attribute,
                        enabled.as_bool().ok_or_else(|| {
                            crate::NativeError::invalid_input("text attributes must be booleans")
                        })?,
                    );
                }
            }
        }
    }
    if let Some(states) = decoration.get("styleStates").and_then(Value::as_object) {
        for (key, value) in states {
            let value = value.as_str().ok_or_else(|| {
                crate::NativeError::invalid_input("style state values must be strings")
            })?;
            view = view.style_state(key.as_str(), value);
        }
    }
    match decoration.get("width").and_then(Value::as_str) {
        Some("fit") => view = view.fit_width(),
        Some("fill") => view = view.fill_width(),
        Some(other) => {
            return Err(crate::NativeError::invalid_input(format!(
                "unknown width rule `{other}`"
            )));
        }
        None => {}
    }
    match decoration.get("height").and_then(Value::as_str) {
        Some("fit") => view = view.fit_height(),
        Some("fill") => view = view.fill_height(),
        Some(other) => {
            return Err(crate::NativeError::invalid_input(format!(
                "unknown height rule `{other}`"
            )));
        }
        None => {}
    }
    if let Some(value) = decoration.get("minWidth") {
        view = view.min_width(u16_value(decoration, "minWidth")?);
        let _ = value;
    }
    if decoration.get("maxWidth").is_some() {
        view = view.max_width(u16_value(decoration, "maxWidth")?);
    }
    if decoration.get("minHeight").is_some() {
        view = view.min_height(u16_value(decoration, "minHeight")?);
    }
    if decoration.get("maxHeight").is_some() {
        view = view.max_height(u16_value(decoration, "maxHeight")?);
    }
    Ok(view)
}

fn color_spec(value: &Value) -> Result<iyon_tui::ColorSpec> {
    if let Some(object) = value.as_object() {
        let kind = object.get("type").and_then(Value::as_str).ok_or_else(|| {
            crate::NativeError::invalid_input("color object type must be a string")
        })?;
        if kind == "ansi" {
            let number = object.get("value").and_then(Value::as_u64).ok_or_else(|| {
                crate::NativeError::invalid_input("ANSI color value must be an integer")
            })?;
            return Ok(iyon_tui::ColorSpec::ansi(u8::try_from(number).map_err(
                |_| crate::NativeError::invalid_input("ANSI color value must fit in u8"),
            )?));
        }
        return Err(crate::NativeError::invalid_input(format!(
            "unknown color object type `{kind}`"
        )));
    }
    let value = value.as_str().ok_or_else(|| {
        crate::NativeError::invalid_input("color must be a string or ANSI color object")
    })?;
    if let Some(value) = value.strip_prefix("theme:") {
        return Ok(iyon_tui::ColorSpec::theme(value));
    }
    if let Some(value) = value.strip_prefix("ansi:") {
        return Ok(iyon_tui::ColorSpec::ansi(value.parse::<u8>().map_err(
            |_| crate::NativeError::invalid_input("ANSI color must fit in u8"),
        )?));
    }
    if let Some(value) = value.strip_prefix('#') {
        if value.len() == 6 {
            let r = u8::from_str_radix(&value[0..2], 16).map_err(|_| {
                crate::NativeError::invalid_input("RGB color must contain hexadecimal bytes")
            })?;
            let g = u8::from_str_radix(&value[2..4], 16).map_err(|_| {
                crate::NativeError::invalid_input("RGB color must contain hexadecimal bytes")
            })?;
            let b = u8::from_str_radix(&value[4..6], 16).map_err(|_| {
                crate::NativeError::invalid_input("RGB color must contain hexadecimal bytes")
            })?;
            return Ok(iyon_tui::ColorSpec::rgb(r, g, b));
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
        _ => {
            return Err(crate::NativeError::invalid_input(format!(
                "unknown color `{value}`"
            )));
        }
    };
    Ok(iyon_tui::ColorSpec::named(color))
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
            "gap": 0,
            "children": [
                {"kind": "normal", "child": {"type": "text", "spans": [{"text": "one"}]}},
                {"kind": "normal", "child": {"type": "row", "gap": 0, "children": [{"kind": "normal", "child": {"type": "text", "spans": [{"text": "two"}]}}]}}
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
        let stream = NativeTextStream::new(None);
        stream.update("first".into()).unwrap();
        stream.seal().unwrap();
        assert!(stream.update("late".into()).is_err());
    }
}

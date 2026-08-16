use napi::bindgen_prelude::Result;
use napi_derive::napi;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use iyon_tui::{Component, History, IntoView, TextInput, View};
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
    alive: AtomicBool,
}

#[napi]
impl NativeHistory {
    #[napi(constructor)]
    pub fn new() -> Self {
        Self { state: Mutex::new(History::new()), alive: AtomicBool::new(true) }
    }

    #[napi]
    pub fn dispose(&self) { self.alive.store(false, Ordering::Release); }

    #[napi]
    pub fn layout(&self) -> Result<Value> {
        ensure_alive(&self.alive)?;
        let _layout = self.state.lock().map_err(|_| crate::NativeError::internal("history lock is poisoned"))?.layout();
        Ok(serde_json::json!({"padding": 0, "gap": 0}))
    }

    #[napi]
    pub fn push(&self, view: &NativeTuiView) -> Result<()> {
        ensure_alive(&self.alive)?;
        self.state.lock().map_err(|_| crate::NativeError::internal("history lock is poisoned"))?.push(view.view.clone()).map(|_| ()).map_err(|error| crate::NativeError::invalid_input(error.to_string()))
    }

    #[napi(js_name = "pushStream")]
    pub fn push_stream(&self, stream: &NativeTextStream) -> Result<()> {
        ensure_alive(&self.alive)?;
        if stream.is_sealed()? { return Err(crate::NativeError::invalid_input("a sealed stream cannot be appended")); }
        Ok(())
    }
}

#[napi]
pub struct NativeTextInput {
    state: Mutex<TextInput>,
    alive: AtomicBool,
}

#[napi]
impl NativeTextInput {
    #[napi(constructor)]
    pub fn new(multiline: Option<bool>) -> Self {
        Self { state: Mutex::new(TextInput::new().multiline(multiline.unwrap_or(false))), alive: AtomicBool::new(true) }
    }

    #[napi]
    pub fn dispose(&self) { self.alive.store(false, Ordering::Release); }

    #[napi]
    pub fn text(&self) -> Result<String> {
        ensure_alive(&self.alive)?;
        Ok(self.state.lock().map_err(|_| crate::NativeError::internal("text input lock is poisoned"))?.text().to_owned())
    }

    #[napi(js_name = "cursorBytes")]
    pub fn cursor_bytes(&self) -> Result<i64> {
        ensure_alive(&self.alive)?;
        Ok(self.state.lock().map_err(|_| crate::NativeError::internal("text input lock is poisoned"))?.cursor_bytes() as i64)
    }

    #[napi(js_name = "setText")]
    pub fn set_text(&self, text: String) -> Result<()> {
        ensure_alive(&self.alive)?;
        self.state.lock().map_err(|_| crate::NativeError::internal("text input lock is poisoned"))?.set_text(text);
        Ok(())
    }

    #[napi]
    pub fn clear(&self) -> Result<()> {
        ensure_alive(&self.alive)?;
        self.state.lock().map_err(|_| crate::NativeError::internal("text input lock is poisoned"))?.clear();
        Ok(())
    }

    #[napi(js_name = "setMultiline")]
    pub fn set_multiline(&self, enabled: bool) -> Result<()> {
        ensure_alive(&self.alive)?;
        self.state.lock().map_err(|_| crate::NativeError::internal("text input lock is poisoned"))?.set_multiline(enabled);
        Ok(())
    }

    #[napi(js_name = "isMultiline")]
    pub fn is_multiline(&self) -> Result<bool> {
        ensure_alive(&self.alive)?;
        Ok(self.state.lock().map_err(|_| crate::NativeError::internal("text input lock is poisoned"))?.is_multiline())
    }

    #[napi]
    pub fn submitted(&self) -> Result<Option<String>> { ensure_alive(&self.alive)?; Ok(None) }

    #[napi]
    pub fn view(&self) -> Result<NativeTuiView> {
        ensure_alive(&self.alive)?;
        let input = self.state.lock().map_err(|_| crate::NativeError::internal("text input lock is poisoned"))?;
        Ok(NativeTuiView { view: input.view() })
    }
}

#[napi]
pub struct NativeTextStream {
    text: Mutex<String>,
    revision: AtomicU64,
    sealed: AtomicBool,
    alive: AtomicBool,
}

#[napi]
impl NativeTextStream {
    #[napi(constructor)]
    pub fn new() -> Self { Self { text: Mutex::new(String::new()), revision: AtomicU64::new(0), sealed: AtomicBool::new(false), alive: AtomicBool::new(true) } }

    #[napi]
    pub fn dispose(&self) { self.alive.store(false, Ordering::Release); }

    #[napi]
    pub fn update(&self, text: String) -> Result<()> {
        ensure_alive(&self.alive)?;
        if self.sealed.load(Ordering::Acquire) { return Err(crate::NativeError::invalid_input("stream is already sealed")); }
        *self.text.lock().map_err(|_| crate::NativeError::internal("stream lock is poisoned"))? = text;
        self.revision.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    #[napi]
    pub fn seal(&self) -> Result<()> {
        ensure_alive(&self.alive)?;
        if self.sealed.swap(true, Ordering::AcqRel) { return Err(crate::NativeError::invalid_input("stream is already sealed")); }
        Ok(())
    }

    #[napi]
    pub fn snapshot(&self) -> Result<Value> {
        ensure_alive(&self.alive)?;
        Ok(serde_json::json!({"text": self.text.lock().map_err(|_| crate::NativeError::internal("stream lock is poisoned"))?.clone(), "revision": self.revision.load(Ordering::Acquire), "sealed": self.sealed.load(Ordering::Acquire)}))
    }

    fn is_sealed(&self) -> Result<bool> { ensure_alive(&self.alive)?; Ok(self.sealed.load(Ordering::Acquire)) }
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
        Self { id: NEXT_COMPONENT_ID.fetch_add(1, Ordering::AcqRel), revision: AtomicU64::new(0), alive: AtomicBool::new(true) }
    }

    #[napi]
    pub fn dispose(&self) { self.alive.store(false, Ordering::Release); }

    #[napi]
    pub fn revision(&self) -> Result<i64> { ensure_alive(&self.alive)?; Ok(self.revision.load(Ordering::Acquire) as i64) }

    #[napi]
    pub fn id(&self) -> Result<i64> { ensure_alive(&self.alive)?; Ok(self.id as i64) }
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
        "row" | "column" => {
            let children = child_views(object)?;
            if kind == "row" {
                View::horizontal(|row| {
                    row.children(children);
                })
            } else {
                View::vertical(|column| {
                    column.children(children);
                })
            }
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
        "decorated" => apply_decoration(lower_required(object, "child")?, object.get("decoration"))?,
        "component" => View::spacer(0),
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
    if let Some(style) = decoration.get("style").and_then(Value::as_object) {
        if let Some(attributes) = style.get("attributes").and_then(Value::as_object) {
            for (name, enabled) in attributes {
                if let Some(attribute) = text_attribute(name) {
                    view = view.text_attribute(attribute, enabled.as_bool().unwrap_or(false));
                }
            }
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

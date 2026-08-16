use napi::bindgen_prelude::Result;
use napi_derive::napi;

use iyon_tui::{IntoView, View};
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
}

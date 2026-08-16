use napi::bindgen_prelude::Result;
use napi_derive::napi;

use iyon_tui::{IntoView, View};

/// Link/surface probe only: construct one owned public TUI value and discard
/// it. The native bridge must not duplicate or serialize the TUI renderer.
#[napi(js_name = "tuiSmoke")]
pub fn tui_smoke() -> Result<String> {
    let _view = View::text("iyon-tui/t1").into_view();
    Ok("iyon-tui/t1".to_owned())
}

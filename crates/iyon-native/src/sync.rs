use napi::bindgen_prelude::{Buffer, Result};
use napi_derive::napi;
use serde_json::Value;

use crate::NativeError;

/// T1's stable marker proves Bun loaded this exact native bridge.
#[napi(js_name = "nativeVersion")]
pub fn native_version() -> String {
    "iyon-native/t1".to_owned()
}

/// Convert at the synchronous boundary so the addon owns the complete JSON
/// value before any future is introduced. N-API conversion failures are
/// typed errors; no JS value or Rust borrow crosses an async boundary.
#[napi(js_name = "echoJson")]
pub fn echo_json(value: Value) -> Result<Value> {
    Ok(value)
}

#[napi(js_name = "echoString")]
pub fn echo_string(value: String) -> Result<String> {
    Ok(value)
}

#[napi(js_name = "echoBuffer")]
pub fn echo_buffer(value: Buffer) -> Result<Buffer> {
    let owned = value.to_vec();
    if owned.len() > isize::MAX as usize {
        return Err(NativeError::invalid_input("buffer is too large"));
    }
    Ok(Buffer::from(owned))
}

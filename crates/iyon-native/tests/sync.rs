use iyon_native::{echo_buffer, echo_json, echo_string, native_version, tui_smoke};
use napi::bindgen_prelude::Buffer;
use serde_json::json;

#[test]
fn json_round_trip_preserves_nested_values() {
    let value = json!({
        "object": {"array": [null, true, 42, "text"]},
        "negative": -3.5
    });

    assert_eq!(
        echo_json(value.clone()).expect("JSON conversion should succeed"),
        value
    );
}

#[test]
fn large_string_conversion_is_owned_and_lossless() {
    let value = "x".repeat(1024 * 1024);
    assert_eq!(
        echo_string(value.clone()).expect("string conversion should succeed"),
        value
    );
}

#[test]
fn buffer_conversion_copies_bytes() {
    let value = Buffer::from(vec![0, 1, 2, 255]);
    let echoed = echo_buffer(value).expect("buffer conversion should succeed");
    assert_eq!(echoed.as_ref(), &[0, 1, 2, 255]);
}

#[test]
fn native_and_tui_markers_are_stable() {
    assert_eq!(native_version(), "iyon-native/t1");
    assert_eq!(
        tui_smoke().expect("TUI link probe should succeed"),
        "iyon-tui/t1"
    );
}

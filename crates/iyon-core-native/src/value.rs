use serde_json::Value;

use crate::NativeError;

pub(crate) fn object(
    value: Value,
    name: &str,
) -> Result<serde_json::Map<String, Value>, napi::Error> {
    value
        .as_object()
        .cloned()
        .ok_or_else(|| NativeError::invalid_input(format!("{name} must be an object")))
}

pub(crate) fn required_string(
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<String, napi::Error> {
    object
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| NativeError::invalid_input(format!("{field} must be a string")))
}

pub(crate) fn optional_string(
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<Option<String>, napi::Error> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_str()
            .map(|value| Some(value.to_owned()))
            .ok_or_else(|| NativeError::invalid_input(format!("{field} must be a string"))),
    }
}

pub(crate) fn required_u64(
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<u64, napi::Error> {
    object.get(field).and_then(Value::as_u64).ok_or_else(|| {
        NativeError::invalid_input(format!("{field} must be a non-negative integer"))
    })
}

pub(crate) fn optional_object(
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<serde_json::Map<String, Value>, napi::Error> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(serde_json::Map::new()),
        Some(value) => value
            .as_object()
            .cloned()
            .ok_or_else(|| NativeError::invalid_input(format!("{field} must be an object"))),
    }
}

pub(crate) fn array(
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<Vec<Value>, napi::Error> {
    object
        .get(field)
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| NativeError::invalid_input(format!("{field} must be an array")))
}

pub(crate) fn optional_array(
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<Vec<Value>, napi::Error> {
    match object.get(field) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(value) => value
            .as_array()
            .cloned()
            .ok_or_else(|| NativeError::invalid_input(format!("{field} must be an array"))),
    }
}

pub(crate) fn discriminant(object: &serde_json::Map<String, Value>) -> Result<&str, napi::Error> {
    object
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| NativeError::invalid_input("type must be a string"))
}

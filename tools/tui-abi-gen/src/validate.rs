use std::collections::HashSet;

use serde_json::Map;
use thiserror::Error;

use crate::model::{AbiDocument, EnumSpec};

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("{0}")]
    Invalid(String),
}

pub fn validate(
    document: &AbiDocument,
    bridge_schema: &Map<String, serde_json::Value>,
) -> Result<(), ValidationError> {
    if document.abi.name.is_empty() || !is_snake_case(&document.abi.name) {
        return invalid("abi.name must be a non-empty snake_case identifier");
    }
    if document.abi.version == 0 || document.abi.semantic_schema == 0 {
        return invalid("abi.version and abi.semantic_schema must be non-zero");
    }
    if document.abi.minimum_bun != "1.4.0" {
        return invalid("abi.minimum_bun must be exactly 1.4.0 for Tranche 1");
    }
    if document.abi.result_encoding != "u32_high_bit_status" {
        return invalid("abi.result_encoding must be u32_high_bit_status");
    }

    let mut handle_names = HashSet::new();
    for handle in &document.handles {
        if !is_pascal_case(&handle.name) {
            return invalid(format!("handle {} must be PascalCase", handle.name));
        }
        if !handle_names.insert(handle.name.as_str()) {
            return invalid(format!("duplicate handle {}", handle.name));
        }
        if handle.rust.is_empty() || handle.typescript.is_empty() || handle.lifetime.is_empty() {
            return invalid(format!("handle {} has an empty ABI property", handle.name));
        }
        if handle.kind.is_some() != handle.valid.is_some() {
            return invalid(format!(
                "handle {} must specify both kind and valid, or neither",
                handle.name
            ));
        }
    }

    let mut enum_names = HashSet::new();
    for enum_spec in &document.enums {
        validate_enum(enum_spec, bridge_schema)?;
        if !enum_names.insert(enum_spec.name.as_str()) {
            return invalid(format!("duplicate enum {}", enum_spec.name));
        }
    }

    let mut function_names = HashSet::new();
    for function in &document.functions {
        if !is_snake_case(&function.name) {
            return invalid(format!("function {} must be snake_case", function.name));
        }
        if !function_names.insert(function.name.as_str()) {
            return invalid(format!("duplicate function {}", function.name));
        }
        if function.family.is_empty()
            || function.hotness.is_empty()
            || function.implementation.is_empty()
            || function.fallback.is_empty()
            || function.ownership.is_empty()
            || function.borrow_duration.is_empty()
            || function.thread_affinity.is_empty()
            || function.benchmark_registration.is_empty()
        {
            return invalid(format!(
                "function {} has an empty ABI property",
                function.name
            ));
        }
        if !is_snake_case(&function.implementation) {
            return invalid(format!(
                "implementation {} must be snake_case",
                function.implementation
            ));
        }
        if function.borrow_duration != "call"
            || function.thread_affinity != "owner_thread"
            || function.max_input_count > 1_000_000
            || function.max_buffer_bytes > 16 * 1024 * 1024
            || function
                .arity_specializations
                .iter()
                .any(|arity| *arity > 16)
        {
            return invalid(format!(
                "function {} has unsupported ownership, lifetime, thread, or bound policy",
                function.name
            ));
        }
        if !matches!(
            function.return_type.as_str(),
            "u32" | "i32" | "ViewRefResult" | "status_only" | "native_ref_result"
        ) {
            return invalid(format!(
                "function {} has unsupported return type {}",
                function.name, function.return_type
            ));
        }

        let mut argument_names = HashSet::new();
        let mut variable_buffers = 0;
        for argument in &function.args {
            if !is_snake_case(&argument.name) {
                return invalid(format!(
                    "argument {}.{} must be snake_case",
                    function.name, argument.name
                ));
            }
            if !argument_names.insert(argument.name.as_str()) {
                return invalid(format!(
                    "duplicate argument {}.{}",
                    function.name, argument.name
                ));
            }
            validate_type(&argument.type_name, document, function.name.as_str())?;
            if !matches!(
                argument.lowering.as_str(),
                "u8" | "u16"
                    | "u32"
                    | "i32"
                    | "f32"
                    | "f64"
                    | "node_id_pair"
                    | "native_ref"
                    | "runtime_ptr"
                    | "host_ptr"
                    | "buffer"
                    | "buffer_length"
                    | "buffer_used"
                    | "cstring_ephemeral"
                    | "pod_slice"
                    | "status_only"
                    | "native_ref_result"
            ) {
                return invalid(format!(
                    "argument {}.{} has unsupported lowering {}",
                    function.name, argument.name, argument.lowering
                ));
            }
            if matches!(argument.lowering.as_str(), "buffer" | "pod_slice") {
                variable_buffers += 1;
            }
            if let Some(length_of) = &argument.buffer_length_of {
                if argument.lowering != "buffer_length" {
                    return invalid(format!(
                        "{}.{} sets buffer_length_of but is not a buffer_length lowering",
                        function.name, argument.name
                    ));
                }
                if !argument_names.contains(length_of.as_str()) {
                    return invalid(format!(
                        "{}.{} refers to unknown buffer {}",
                        function.name, argument.name, length_of
                    ));
                }
            }
        }
        if variable_buffers > 1 {
            return invalid(format!(
                "function {} has more than one variable buffer in tranche 1",
                function.name
            ));
        }
        if variable_buffers > 0 && function.max_buffer_bytes == 0 {
            return invalid(format!(
                "buffer function {} must declare max_buffer_bytes",
                function.name
            ));
        }
    }

    Ok(())
}

fn validate_enum(
    enum_spec: &EnumSpec,
    bridge_schema: &Map<String, serde_json::Value>,
) -> Result<(), ValidationError> {
    if !is_pascal_case(&enum_spec.name) {
        return invalid(format!("enum {} must be PascalCase", enum_spec.name));
    }
    if enum_spec.repr != "u32" {
        return invalid(format!("enum {} must use u32 in tranche 1", enum_spec.name));
    }
    if enum_spec.values.is_empty() {
        return invalid(format!(
            "enum {} must define at least one value",
            enum_spec.name
        ));
    }
    let mut names = HashSet::new();
    for value in &enum_spec.values {
        if !is_pascal_case(&value.name) || !names.insert(value.name.as_str()) {
            return invalid(format!(
                "enum {} has an invalid or duplicate value {}",
                enum_spec.name, value.name
            ));
        }
        let Some(number) = bridge_schema
            .get(&value.source_key)
            .and_then(serde_json::Value::as_u64)
        else {
            return invalid(format!(
                "enum {} value {} does not resolve integer bridge key {}",
                enum_spec.name, value.name, value.source_key
            ));
        };
        if number > u32::MAX as u64 {
            return invalid(format!("bridge key {} does not fit u32", value.source_key));
        }
    }
    Ok(())
}

fn validate_type(
    type_name: &str,
    document: &AbiDocument,
    function_name: &str,
) -> Result<(), ValidationError> {
    let primitive = matches!(type_name, "u8" | "u16" | "u32" | "i32" | "f32" | "f64");
    let builtin = matches!(type_name, "u32[]" | "AxisChildInputV1[]");
    let handle = document.handles.iter().any(|item| item.name == type_name);
    let enum_type = document.enums.iter().any(|item| item.name == type_name);
    if !(primitive || builtin || handle || enum_type) {
        return invalid(format!(
            "function {} refers to unknown type {}",
            function_name, type_name
        ));
    }
    Ok(())
}

fn is_snake_case(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
        && !value.starts_with('_')
        && !value.ends_with('_')
}

fn is_pascal_case(value: &str) -> bool {
    value
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_uppercase())
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
}

fn invalid(message: impl Into<String>) -> Result<(), ValidationError> {
    Err(ValidationError::Invalid(message.into()))
}

use serde_json::Map;

use crate::{
    model::{AbiDocument, ArgumentSpec},
    render_manifest::banner,
};

pub fn header(
    document: &AbiDocument,
    bridge_schema: &Map<String, serde_json::Value>,
    schema_hash: &str,
    generator_hash: &str,
) -> String {
    let mut output = banner(schema_hash, generator_hash).replace("//", "/*");
    output = output.replace('\n', " */\n");
    output.push_str("#ifndef IYON_VIEW_ABI_H\n#define IYON_VIEW_ABI_H\n\n#include <stddef.h>\n#include <stdint.h>\n\n");
    output.push_str(&format!("#define IYON_VIEW_ABI_NAME \"{}\"\n#define IYON_VIEW_ABI_VERSION {}\n#define IYON_VIEW_SEMANTIC_SCHEMA_VERSION {}\n#define IYON_VIEW_RESULT_ERROR_BIT UINT32_C(0x80000000)\n\n", document.abi.name, document.abi.version, document.abi.semantic_schema));
    output.push_str("typedef struct NativeViewRuntime NativeViewRuntime;\ntypedef struct NativeHost NativeHost;\ntypedef struct AxisChildInputV1 { uint32_t track_word; uint32_t child_ref; } AxisChildInputV1;\n\n");
    for enum_spec in &document.enums {
        output.push_str(&format!("typedef enum {} {{\n", enum_spec.name));
        for value in &enum_spec.values {
            let number = bridge_schema
                .get(&value.source_key)
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            output.push_str(&format!(
                "    {}_{} = UINT32_C({}),\n",
                enum_spec.name, value.name, number
            ));
        }
        output.push_str(&format!("}} {};\n\n", enum_spec.name));
    }
    for function in &document.functions {
        output.push_str(&format!(
            "{} iyon_{}_v1({});\n\n",
            c_return(function.return_type.as_str()),
            function.name,
            c_arguments(&function.args, document)
        ));
    }
    output.push_str("#endif /* IYON_VIEW_ABI_H */\n");
    output
}

fn c_arguments(arguments: &[ArgumentSpec], document: &AbiDocument) -> String {
    arguments
        .iter()
        .flat_map(|argument| {
            let mut values = vec![format!("{} {}", c_type(argument, document), argument.name)];
            if matches!(argument.lowering.as_str(), "buffer" | "pod_slice") {
                values.push(format!("size_t {}_capacity_bytes", argument.name));
            }
            values
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn c_type(argument: &ArgumentSpec, document: &AbiDocument) -> String {
    match argument.lowering.as_str() {
        "runtime_ptr" => "NativeViewRuntime *".to_owned(),
        "host_ptr" => "NativeHost *".to_owned(),
        "buffer" if argument.type_name == "u32[]" => "const uint32_t *".to_owned(),
        "buffer" => "const uint8_t *".to_owned(),
        "pod_slice" => "const AxisChildInputV1 *".to_owned(),
        "i32" | "status_only" => "int32_t".to_owned(),
        "u8" => "uint8_t".to_owned(),
        "u16" => "uint16_t".to_owned(),
        "f32" => "float".to_owned(),
        "f64" => "double".to_owned(),
        _ if document
            .enums
            .iter()
            .any(|item| item.name == argument.type_name) =>
        {
            "uint32_t".to_owned()
        }
        _ => "uint32_t".to_owned(),
    }
}

fn c_return(return_type: &str) -> &'static str {
    match return_type {
        "i32" | "status_only" => "int32_t",
        "u32" | "ViewRefResult" | "native_ref_result" => "uint32_t",
        other => panic!("unsupported generated C return {other}"),
    }
}

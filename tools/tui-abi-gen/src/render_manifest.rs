use askama::Template;
use serde_json::{Value, json};

use crate::model::AbiDocument;

const GENERATOR_FINGERPRINT: &str = "tui-abi-gen:tranche-1:v1";

#[derive(Template)]
#[template(path = "generated_banner.txt")]
struct GeneratedBanner<'a> {
    schema_hash: &'a str,
    generator_hash: &'a str,
}

pub fn generator_hash() -> String {
    blake3::hash(GENERATOR_FINGERPRINT.as_bytes())
        .to_hex()
        .to_string()
}

pub fn banner(schema_hash: &str, generator_hash: &str) -> String {
    let rendered = GeneratedBanner {
        schema_hash,
        generator_hash,
    }
    .render()
    .expect("generated banner template is valid");
    if rendered.ends_with('\n') {
        rendered
    } else {
        format!("{rendered}\n")
    }
}

pub fn manifest(
    document: &AbiDocument,
    schema_hash: &str,
    generator_hash: &str,
    output_paths: &[&str],
) -> String {
    let functions: Vec<Value> = document
        .functions
        .iter()
        .map(|function| {
            json!({
                "name": function.name,
                "family": function.family,
                "hotness": function.hotness,
                "implementation": function.implementation,
                "fallback": function.fallback,
                "return": function.return_type,
                "args": function.args.iter().map(|argument| json!({
                    "name": argument.name,
                    "type": argument.type_name,
                    "lowering": argument.lowering,
                    "buffer_length_of": argument.buffer_length_of,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let enums: Vec<Value> = document
        .enums
        .iter()
        .map(|enum_spec| {
            json!({
                "name": enum_spec.name,
                "source": enum_spec.source,
                "repr": enum_spec.repr,
                "values": enum_spec.values.iter().map(|value| json!({
                    "name": value.name,
                    "source_key": value.source_key,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let value = json!({
        "abi": {
            "name": document.abi.name,
            "version": document.abi.version,
            "semantic_schema": document.abi.semantic_schema,
            "minimum_bun": document.abi.minimum_bun,
            "result_encoding": document.abi.result_encoding,
        },
        "schema_blake3": schema_hash,
        "generator_blake3": generator_hash,
        "handles": document.handles,
        "enums": enums,
        "functions": functions,
        "generated_outputs": output_paths,
    });
    serde_json::to_string_pretty(&value).expect("ABI manifest is serializable") + "\n"
}

pub fn human_reference(document: &AbiDocument, schema_hash: &str, generator_hash: &str) -> String {
    let mut output = String::new();
    output.push_str(&format!("<!-- DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml. schema_blake3 = {schema_hash}; generator_blake3 = {generator_hash} -->\n\n"));
    output.push_str("# PERF-11 generated ABI reference\n\n");
    output.push_str("> This file is generated. Do not edit it directly.\n\n");
    output.push_str(&format!("- Schema BLAKE3: `{schema_hash}`\n- Generator BLAKE3: `{generator_hash}`\n- ABI: `{}` v{}\n- Semantic schema: v{}\n- Minimum Bun: `{}`\n\n", document.abi.name, document.abi.version, document.abi.semantic_schema, document.abi.minimum_bun));
    output.push_str(
        "## Handles\n\n| Name | Rust | TypeScript | Lifetime | Kind |\n|---|---|---|---|---|\n",
    );
    for handle in &document.handles {
        output.push_str(&format!(
            "| `{}` | `{}` | `{}` | `{}` | `{}` |\n",
            handle.name,
            handle.rust,
            handle.typescript,
            handle.lifetime,
            handle.kind.as_deref().unwrap_or("-")
        ));
    }
    output.push_str("\n## Enums\n\n");
    for enum_spec in &document.enums {
        output.push_str(&format!(
            "### `{}`\n\n| Value | Bridge key |\n|---|---|\n",
            enum_spec.name
        ));
        for value in &enum_spec.values {
            output.push_str(&format!("| `{}` | `{}` |\n", value.name, value.source_key));
        }
        output.push('\n');
    }
    output.push_str(
        "## Functions\n\n| Name | Family | Hotness | Return | Fallback |\n|---|---|---|---|---|\n",
    );
    for function in &document.functions {
        output.push_str(&format!(
            "| `{}` | `{}` | `{}` | `{}` | `{}` |\n",
            function.name,
            function.family,
            function.hotness,
            function.return_type,
            function.fallback
        ));
    }
    output.push_str("\n");
    output
}

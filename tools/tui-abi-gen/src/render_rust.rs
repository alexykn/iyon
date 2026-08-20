use std::io::Write;
use std::process::{Command, Stdio};

use quote::quote;
use serde_json::Map;

use crate::{
    model::{AbiDocument, ArgumentSpec},
    render_manifest::banner,
};

pub fn types(
    document: &AbiDocument,
    bridge_schema: &Map<String, serde_json::Value>,
    schema_hash: &str,
    generator_hash: &str,
) -> String {
    let mut output = banner(schema_hash, generator_hash);
    output.push_str("//! Canonical pointer-free ABI types and constants.\n\n");
    output.push_str(&format!("pub const ABI_NAME: &str = {:?};\npub const ABI_VERSION: u32 = {};\npub const SEMANTIC_SCHEMA_VERSION: u32 = {};\npub const MINIMUM_BUN: &str = {:?};\npub const QUALIFIED_BUN: &str = {:?};\npub const RESULT_ERROR_BIT: u32 = 0x8000_0000;\n\n", document.abi.name, document.abi.version, document.abi.semantic_schema, document.abi.minimum_bun, document.abi.qualified_bun));
    output.push_str("pub type ViewRefResult = u32;\n\n");
    for pod in &document.pods {
        output.push_str("#[repr(C)]\n#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]\n");
        output.push_str(&format!("pub struct {} {{\n", pod.name));
        for field in &pod.fields {
            output.push_str(&format!(
                "    pub {}: {},\n",
                field.name,
                rust_type(&field.type_name)
            ));
        }
        output.push_str("}\n\n");
        output.push_str(&format!(
            "static_assertions::const_assert_eq!(::core::mem::size_of::<{}>(), {});\nstatic_assertions::const_assert_eq!(::core::mem::align_of::<{}>(), {});\n\n",
            pod.name, pod.size, pod.name, pod.align
        ));
    }
    output.push_str("#[repr(C)]\n#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]\npub struct NativeViewAbiHeader {\n    pub magic: u32,\n    pub abi_version: u32,\n    pub semantic_version: u32,\n    pub alive: u32,\n}\n\nstatic_assertions::const_assert_eq!(::core::mem::size_of::<NativeViewAbiHeader>(), 16);\n\n");
    for enum_spec in &document.enums {
        output.push_str("#[repr(u32)]\n#[derive(Clone, Copy, Debug, Eq, PartialEq)]\n");
        output.push_str(&format!("pub enum {} {{\n", enum_spec.name));
        for value in &enum_spec.values {
            let number = bridge_schema
                .get(&value.source_key)
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            output.push_str(&format!("    {} = {},\n", value.name, number));
        }
        output.push_str("}\n\n");
        for value in &enum_spec.values {
            let number = bridge_schema
                .get(&value.source_key)
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            output.push_str(&format!(
                "static_assertions::const_assert_eq!({}::{} as u32, {});\n",
                enum_spec.name, value.name, number
            ));
        }
        output.push('\n');
    }
    format_rust(output)
}

pub fn exports(document: &AbiDocument, schema_hash: &str, generator_hash: &str) -> String {
    let mut source = banner(schema_hash, generator_hash);
    source.push_str(
        "// Generated C ABI wrappers. Semantic implementations are supplied by the next tranche.\n",
    );
    source.push_str(&format!("{}\n", export_imports(document)));
    source.push_str("pub mod generated_impls {\n");
    source.push_str(&format!("    {}\n", export_imports(document)));

    for function in &document.functions {
        source.push_str(&format!(
            "    unsafe extern \"Rust\" {{\n        pub fn {}({}) -> {};\n    }}\n",
            function.implementation,
            rust_arguments(&function.args, document),
            rust_type(function.return_type.as_str())
        ));
    }
    source.push_str("}\n\n");
    for function in &document.functions {
        source.push_str("#[unsafe(no_mangle)]\n");
        source.push_str(&format!(
            "pub unsafe extern \"C\" fn iyon_{}_v1({}) -> {} {{\n",
            function.name,
            rust_arguments(&function.args, document),
            rust_type(function.return_type.as_str())
        ));
        source.push_str(&format!(
            "    unsafe {{ generated_impls::{}({}) }}\n",
            function.implementation,
            rust_call_arguments(&function.args)
        ));
        source.push_str("}\n\n");
    }
    format_rust(source)
}

pub fn table(document: &AbiDocument, schema_hash: &str, generator_hash: &str) -> String {
    let mut output = banner(schema_hash, generator_hash);
    output.push_str("#[derive(Clone, Copy, Debug)]\npub struct FunctionDescriptor {\n    pub name: &'static str,\n    pub symbol: &'static str,\n    pub family: &'static str,\n    pub hotness: &'static str,\n    pub fallback: &'static str,\n    pub ownership: &'static str,\n    pub borrow_duration: &'static str,\n    pub thread_affinity: &'static str,\n    pub may_allocate_native_memory: bool,\n    pub mutates_host_state: bool,\n    pub max_buffer_bytes: u64,\n    pub max_input_count: u32,\n    pub benchmark_registration: &'static str,\n}\n\n");
    output.push_str("pub static FUNCTIONS: &[FunctionDescriptor] = &[\n");
    for function in &document.functions {
        output.push_str(&format!(
            "    FunctionDescriptor {{\n        name: {:?},\n        symbol: {:?},\n        family: {:?},\n        hotness: {:?},\n        fallback: {:?},\n        ownership: {:?},\n        borrow_duration: {:?},\n        thread_affinity: {:?},\n        may_allocate_native_memory: {},\n        mutates_host_state: {},\n        max_buffer_bytes: {},\n        max_input_count: {},\n        benchmark_registration: {:?},\n    }},\n",
            function.name,
            format!("iyon_{}_v1", function.name),
            function.family,
            function.hotness,
            function.fallback,
            function.ownership,
            function.borrow_duration,
            function.thread_affinity,
            function.may_allocate_native_memory,
            function.mutates_host_state,
            function.max_buffer_bytes,
            function.max_input_count,
            function.benchmark_registration
        ));
    }
    output.push_str("];\n\npub const FUNCTION_COUNT: usize = FUNCTIONS.len();\n");
    output
}

fn export_imports(document: &AbiDocument) -> String {
    let pod_names = document
        .pods
        .iter()
        .filter(|pod| {
            document.functions.iter().any(|function| {
                function.args.iter().any(|argument| {
                    argument.lowering == "pod_slice"
                        && argument.type_name.strip_suffix("[]") == Some(pod.name.as_str())
                })
            })
        })
        .map(|pod| pod.name.as_str())
        .collect::<Vec<_>>();
    if pod_names.is_empty() {
        "use super::NativeViewRuntime;".to_owned()
    } else {
        format!(
            "use super::{{{}, {}}};",
            "NativeViewRuntime",
            pod_names.join(", ")
        )
    }
}

fn rust_arguments(arguments: &[ArgumentSpec], document: &AbiDocument) -> String {
    let mut rendered = Vec::new();
    for argument in arguments {
        if argument.lowering == "node_id_pair" {
            rendered.push(format!("{}_low: u32", argument.name));
            rendered.push(format!("{}_high: u32", argument.name));
        } else {
            rendered.push(format!(
                "{}: {}",
                argument.name,
                rust_type_for_argument(argument, document)
            ));
        }
    }
    rendered.join(", ")
}

fn rust_type_for_argument(argument: &ArgumentSpec, document: &AbiDocument) -> String {
    match argument.lowering.as_str() {
        "runtime_ptr" => "*mut NativeViewRuntime".to_owned(),
        "host_ptr" => "*mut NativeHost".to_owned(),
        "native_ref" | "node_id_pair" | "native_ref_result" => "u32".to_owned(),
        "buffer" if argument.type_name == "u32[]" => "*const u32".to_owned(),
        "buffer" => "*const u8".to_owned(),
        "pod_slice" => argument
            .type_name
            .strip_suffix("[]")
            .map_or_else(|| "*const u8".to_owned(), |name| format!("*const {name}")),
        "buffer_length" => "usize".to_owned(),
        _ => rust_type(&type_name(argument, document)),
    }
}

fn type_name(argument: &ArgumentSpec, document: &AbiDocument) -> String {
    if document
        .handles
        .iter()
        .any(|handle| handle.name == argument.type_name)
        || document
            .enums
            .iter()
            .any(|enum_spec| enum_spec.name == argument.type_name)
    {
        return "u32".to_owned();
    }
    argument.type_name.clone()
}

fn rust_call_arguments(arguments: &[ArgumentSpec]) -> String {
    arguments
        .iter()
        .flat_map(|argument| {
            if argument.lowering == "node_id_pair" {
                return vec![
                    format!("{}_low", argument.name),
                    format!("{}_high", argument.name),
                ];
            }
            vec![argument.name.clone()]
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn rust_type(type_name: &str) -> String {
    match type_name {
        "ViewRefResult" | "native_ref_result" | "u32" => "u32".to_owned(),
        "i32" | "status_only" => "i32".to_owned(),
        "u8" => "u8".to_owned(),
        "u16" => "u16".to_owned(),
        "f32" => "f32".to_owned(),
        "f64" => "f64".to_owned(),
        other => other.to_owned(),
    }
}

pub fn layout_tests(document: &AbiDocument, schema_hash: &str, generator_hash: &str) -> String {
    let mut output = banner(schema_hash, generator_hash);
    let pod_imports = document
        .pods
        .iter()
        .filter(|pod| {
            document.functions.iter().any(|function| {
                function.args.iter().any(|argument| {
                    argument.lowering == "pod_slice"
                        && argument.type_name.strip_suffix("[]") == Some(pod.name.as_str())
                })
            })
        })
        .map(|pod| pod.name.as_str())
        .collect::<Vec<_>>();
    let generated_root_imports = if pod_imports.is_empty() {
        String::new()
    } else {
        format!("use generated_types::{{{}}};\n\n", pod_imports.join(", "))
    };
    output.push_str(&format!("#[allow(dead_code)]\nstruct NativeViewRuntime;\n\n#[path = \"../src/generated/view_abi_table.rs\"]\nmod generated;\n#[path = \"../src/generated/view_abi_types.rs\"]\nmod generated_types;\n\n{generated_root_imports}mod generated_exports {{\n    include!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/src/generated/view_abi_exports.rs\"));\n}}\n\n#[test]\nfn generated_function_count_is_stable() {{\n"));
    output.push_str(&format!(
        "    assert_eq!(generated::FUNCTION_COUNT, {});\n",
        document.functions.len()
    ));
    output.push_str(
        "}\n\n#[test]\nfn generated_abi_version_is_one() {\n    assert_eq!(generated_types::ABI_VERSION, 1);\n}\n",
    );
    format_rust(output)
}

fn format_rust(source: String) -> String {
    let body_start = ["\n//!", "\n#[", "\npub "]
        .iter()
        .filter_map(|marker| source.find(marker).map(|index| index + 1))
        .min();
    let Some(body_start) = body_start else {
        return source;
    };
    let (prefix, body) = source.split_at(body_start);
    let formatted = rustfmt_body(body).unwrap_or_else(|| {
        syn::parse_file(body)
            .map(|file| {
                let _tokens: proc_macro2::TokenStream = quote!(#file);
                prettyplease::unparse(&file)
            })
            .unwrap_or_else(|_| body.to_owned())
    });
    format!("{prefix}{}\n", formatted.trim_end())
}

fn rustfmt_body(body: &str) -> Option<String> {
    let mut process = Command::new("rustfmt")
        .args(["--edition", "2024", "--emit", "stdout"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    process.stdin.take()?.write_all(body.as_bytes()).ok()?;
    let output = process.wait_with_output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8(output.stdout).ok())
        .flatten()
}

use crate::{
    model::{AbiDocument, ArgumentSpec},
    render_manifest::banner,
};

pub fn abi_bindings(document: &AbiDocument, schema_hash: &str, generator_hash: &str) -> String {
    let mut output = banner(schema_hash, generator_hash).replace("//", "//");
    output.push_str("import { linkSymbols, type Pointer } from \"bun:ffi\";\n\n");
    output.push_str("export type NativeAbiPointers = {\n");
    for function in &document.functions {
        output.push_str(&format!("  {}: Pointer;\n", camel_case(&function.name)));
    }
    output.push_str("};\n\n");
    output.push_str(
        "export function linkViewAbi(abi: NativeAbiPointers) {\n  return linkSymbols({\n",
    );
    for function in &document.functions {
        output.push_str(&format!(
            "    {}: {{ ptr: abi.{}, args: [{}], returns: {:?} }},\n",
            camel_case(&function.name),
            camel_case(&function.name),
            function
                .args
                .iter()
                .flat_map(|argument| ffi_args(argument))
                .map(|item| format!("{item:?}"))
                .collect::<Vec<_>>()
                .join(", "),
            ffi_return(function.return_type.as_str())
        ));
    }
    output.push_str("  } as const);\n}\n");
    output
}

pub fn calls(document: &AbiDocument, schema_hash: &str, generator_hash: &str) -> String {
    let mut output = banner(schema_hash, generator_hash);
    output.push_str("import type { Pointer } from \"bun:ffi\";\nimport type { linkViewAbi } from \"./view_abi\";\n\n");
    output
        .push_str("export type ViewAbiSymbols = ReturnType<typeof linkViewAbi>[\"symbols\"];\n\n");
    output.push_str("const ERROR_BIT = 0x8000_0000;\n\n");
    output.push_str("function checkedRef(result: number): number {\n  if (result >= ERROR_BIT) throw new Error(`native ABI status 0x${result.toString(16)}`);\n  return result;\n}\n\n");
    for function in &document.functions {
        output.push_str(&format!(
            "export function {}(symbols: ViewAbiSymbols, {}): {} {{\n",
            camel_case(&function.name),
            ts_arguments(&function.args, document),
            ts_return(function.return_type.as_str())
        ));
        let call_args = function
            .args
            .iter()
            .flat_map(|argument| {
                let mut values = vec![argument.name.clone()];
                if matches!(argument.lowering.as_str(), "buffer" | "pod_slice") {
                    values.push(argument.name.clone());
                }
                values
            })
            .collect::<Vec<_>>()
            .join(", ");
        output.push_str(&format!(
            "  const result = symbols.{}({});\n",
            camel_case(&function.name),
            call_args
        ));
        if is_ref_result(function.return_type.as_str()) {
            output.push_str("  return checkedRef(result);\n");
        } else {
            output.push_str("  return result;\n");
        }
        output.push_str("}\n\n");
    }
    output
}

pub fn benchmark_registry(
    document: &AbiDocument,
    schema_hash: &str,
    generator_hash: &str,
) -> String {
    let mut output = banner(schema_hash, generator_hash);
    output.push_str("export type GeneratedAbiBenchmarkCase = {\n  name: string;\n  family: string;\n  hotness: string;\n  scalarArgs: number;\n  hasBuffer: boolean;\n};\n\n");
    output.push_str("export const generatedAbiCases: readonly GeneratedAbiBenchmarkCase[] = [\n");
    for function in &document.functions {
        let scalar_args = function
            .args
            .iter()
            .filter(|argument| {
                !matches!(
                    argument.lowering.as_str(),
                    "buffer" | "pod_slice" | "buffer_length"
                )
            })
            .count();
        let has_buffer = function
            .args
            .iter()
            .any(|argument| matches!(argument.lowering.as_str(), "buffer" | "pod_slice"));
        output.push_str(&format!("  {{ name: {:?}, family: {:?}, hotness: {:?}, scalarArgs: {scalar_args}, hasBuffer: {has_buffer} }},\n", function.name, function.family, function.hotness));
    }
    output.push_str("];\n");
    output
}

pub fn layout_test(document: &AbiDocument, schema_hash: &str, generator_hash: &str) -> String {
    let mut output = banner(schema_hash, generator_hash);
    output.push_str(&format!(
        r#"import {{ expect, test }} from "bun:test";
import manifest from "../../src/tui/generated/view_abi_manifest.json";

test("generated ABI manifest is pinned and ordered", () => {{
  expect(manifest.schema_blake3).toBe("{}");
  expect(manifest.abi.version).toBe(1);
  expect(manifest.functions.map((item) => item.name)).toEqual([
"#,
        schema_hash,
    ));
    for function in &document.functions {
        output.push_str(&format!("    {:?},\n", function.name));
    }
    output.push_str("  ]);\n});\n");
    output
}

fn ffi_args(argument: &ArgumentSpec) -> Vec<&'static str> {
    match argument.lowering.as_str() {
        "runtime_ptr" | "host_ptr" => vec!["ptr"],
        "native_ref" | "node_id_pair" | "u32" | "buffer_used" | "native_ref_result" => vec!["u32"],
        "i32" | "status_only" => vec!["i32"],
        "u8" => vec!["u8"],
        "u16" => vec!["u16"],
        "f32" => vec!["f32"],
        "f64" => vec!["f64"],
        "buffer" | "pod_slice" => vec!["buffer", "buffer_length"],
        "buffer_length" => vec!["buffer_length"],
        "cstring_ephemeral" => vec!["cstring"],
        other => panic!("unsupported generated FFI lowering {other}"),
    }
}

fn ffi_return(return_type: &str) -> &'static str {
    match return_type {
        "i32" | "status_only" => "i32",
        "u32" | "ViewRefResult" | "native_ref_result" => "u32",
        other => panic!("unsupported generated FFI return {other}"),
    }
}

fn ts_arguments(arguments: &[ArgumentSpec], document: &AbiDocument) -> String {
    arguments
        .iter()
        .map(|argument| format!("{}: {}", argument.name, ts_type(argument, document)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn ts_type(argument: &ArgumentSpec, document: &AbiDocument) -> &'static str {
    match argument.lowering.as_str() {
        "runtime_ptr" | "host_ptr" => "Pointer",
        "buffer" | "pod_slice" => "NodeJS.TypedArray | DataView",
        "cstring_ephemeral" => "string",
        _ if document
            .enums
            .iter()
            .any(|item| item.name == argument.type_name) =>
        {
            "number"
        }
        _ => "number",
    }
}

fn ts_return(return_type: &str) -> &'static str {
    match return_type {
        "i32" | "status_only" | "u32" | "ViewRefResult" | "native_ref_result" => "number",
        other => panic!("unsupported generated TS return {other}"),
    }
}

fn is_ref_result(return_type: &str) -> bool {
    matches!(return_type, "ViewRefResult" | "native_ref_result")
}

fn camel_case(value: &str) -> String {
    let mut output = String::new();
    let mut uppercase = false;
    for character in value.chars() {
        if character == '_' {
            uppercase = true;
            continue;
        }
        if uppercase {
            output.extend(character.to_uppercase());
            uppercase = false;
        } else {
            output.push(character);
        }
    }
    output
}

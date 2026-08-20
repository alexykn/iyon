// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = 99cb1472686316689de8d738c78dffa5c60e460d5849a235512a038af55c89e3
// generator_blake3 = c4ab26c23fac3bc43c5b8a4b6c75ca3ba1d79b78b4af58ab322f8fef5d9b6601
#[allow(dead_code)]
struct NativeViewRuntime;

#[path = "../src/generated/view_abi_table.rs"]
mod generated;
#[path = "../src/generated/view_abi_types.rs"]
mod generated_types;

use generated_types::AxisChildInputV1;

mod generated_exports {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/generated/view_abi_exports.rs"
    ));
}

#[test]
fn generated_function_count_is_stable() {
    assert_eq!(generated::FUNCTION_COUNT, 7);
}

#[test]
fn generated_abi_version_is_one() {
    assert_eq!(generated_types::ABI_VERSION, 1);
}

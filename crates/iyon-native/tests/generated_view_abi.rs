// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = f62367d8a4d464a917c4958025990e8a120d58409d4a0a55dc5a888a228f6db7
// generator_blake3 = 0407a3e331cbf8a5af827b2e89fe8ceea30d82c1e7cbf0ad92a0d2c272c336a8
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

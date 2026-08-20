// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = 68e52f9913c6c1252f2a061ff4f942c1b32aac35f32bced41e8f9bdc5b2bacb9
// generator_blake3 = 24d34b5e76bb7302928f251bbf11d78e62dfba0dee9cefe44e46082a1aeedc18
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

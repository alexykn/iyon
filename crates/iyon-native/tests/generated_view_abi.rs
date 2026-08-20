// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = 8cbf2331e1e1c177d2d1ee3e7e98d0e8232324ee3f1cb9039499d4d7f2da58cd
// generator_blake3 = 940295ec578b0cbae431fd55425ab9e421cff5bbcb7250e19aceb66323c817af
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

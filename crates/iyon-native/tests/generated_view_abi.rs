// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = 1ca0fdeba92ffd1a195a4898f5629f1f10f849155f8b8b80b03fe1bd050030a8
// generator_blake3 = 5134bc9ebe5a949bd99ece560feb766e2612a8d4b222c03f6b928e766f625ca3
#[path = "../src/generated/view_abi_table.rs"]
mod generated;
#[path = "../src/generated/view_abi_types.rs"]
mod generated_types;

#[test]
fn generated_function_count_is_stable() {
    assert_eq!(generated::FUNCTION_COUNT, 7);
}

#[test]
fn generated_abi_version_is_one() {
    assert_eq!(generated_types::ABI_VERSION, 1);
}

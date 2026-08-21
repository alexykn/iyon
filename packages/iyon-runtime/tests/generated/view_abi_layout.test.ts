// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = e533d64e5293b56a70b81e67a9aee34c17cdfd0a9d1199420cfcb263b2d0f470
// generator_blake3 = 55f2f1590b18e72152621b4c5272e892f224c5d3b4e4d10e489551129f713903
import { expect, test } from "bun:test";
import manifest from "../../src/tui/generated/view_abi_manifest.json";

test("generated ABI manifest is pinned and ordered", () => {
  expect(manifest.schema_blake3).toBe("e533d64e5293b56a70b81e67a9aee34c17cdfd0a9d1199420cfcb263b2d0f470");
  expect(manifest.abi.version).toBe(1);
  expect(manifest.functions.map((item) => item.name)).toEqual([
    "runtime_noop",
    "view_render_ref",
    "host_render_ref",
    "view_spacer_create",
    "view_text_layout_patch_root",
    "view_common_patch_root",
    "view_axis_create_buffer",
    "view_release_many",
    "view_ref_for_node_id",
  ]);
  expect(manifest.conformance.map((item) => item.name)).toEqual([
    "u8_8",
    "u16_8",
    "u32_8",
    "u32_16",
    "i32_4",
    "f32_4",
    "f64_4",
    "pointer",
    "buffer",
    "cstring",
  ]);
});

test("generated ABI conformance signatures are pinned", () => {
  expect(manifest.conformance.map((item) => [item.name, item.return, item.args])).toEqual([
    ["u8_8", "u32", ["u8", "u8", "u8", "u8", "u8", "u8", "u8", "u8"]],
    ["u16_8", "u32", ["u16", "u16", "u16", "u16", "u16", "u16", "u16", "u16"]],
    ["u32_8", "u32", ["u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32"]],
    ["u32_16", "u32", ["u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32"]],
    ["i32_4", "i32", ["i32", "i32", "i32", "i32"]],
    ["f32_4", "f32", ["f32", "f32", "f32", "f32"]],
    ["f64_4", "f64", ["f64", "f64", "f64", "f64"]],
    ["pointer", "u32", ["ptr"]],
    ["buffer", "u32", ["buffer", "buffer_length"]],
    ["cstring", "u32", ["cstring"]],
  ]);
});

test("generated ABI signatures and POD layouts are pinned", () => {
  expect(manifest.abi.qualified_bun).toBe("1.4.0");
  expect(manifest.abi.result_encoding).toBe("u32_high_bit_status");
  expect(manifest.pods.map((item) => [item.name, item.size, item.align])).toEqual([
    ["AxisChildInputV1", 8, 4],
  ]);
  expect(manifest.functions.map((item) => item.args.map((arg) => arg.lowering))).toEqual([
    ["runtime_ptr"],
    ["runtime_ptr", "native_ref"],
    ["runtime_ptr", "host_ptr", "native_ref"],
    ["runtime_ptr", "u32", "u32", "u32"],
    ["runtime_ptr", "native_ref", "u32", "u32", "u32", "u32"],
    ["runtime_ptr", "native_ref", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "native_ref"],
    ["runtime_ptr", "u32", "u32", "u32", "u32", "pod_slice", "buffer_length", "buffer_used"],
    ["runtime_ptr", "buffer", "buffer_length", "buffer_used"],
    ["runtime_ptr", "u32", "u32"],
  ]);
});

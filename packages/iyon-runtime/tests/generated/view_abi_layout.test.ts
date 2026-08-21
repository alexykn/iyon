// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = b6632774c610ea06e51392e4bd1e333cc9cbbb6f39a7ac4c0addff8052b71193
// generator_blake3 = 18452de0513ba234d9b3eab4afe3301ece61e22b53d7d8d242ef1bd7545f6e69
import { expect, test } from "bun:test";
import manifest from "../../src/tui/generated/view_abi_manifest.json";

test("generated ABI manifest is pinned and ordered", () => {
  expect(manifest.schema_blake3).toBe("b6632774c610ea06e51392e4bd1e333cc9cbbb6f39a7ac4c0addff8052b71193");
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
    "path_root",
    "path_child",
    "view_text_layout_patch_path",
    "view_text_layout_patch_path_d1",
    "view_text_layout_patch_path_d2",
    "view_text_layout_patch_path_d3",
    "view_text_layout_patch_path_d4",
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
    ["runtime_ptr"],
    ["runtime_ptr", "native_ref", "u32", "u32", "u32"],
    ["runtime_ptr", "native_ref", "native_ref", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32"],
    ["runtime_ptr", "native_ref", "native_ref", "u32", "u32", "u32", "u32", "u32", "u32"],
    ["runtime_ptr", "native_ref", "native_ref", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32"],
    ["runtime_ptr", "native_ref", "native_ref", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32"],
    ["runtime_ptr", "native_ref", "native_ref", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32"],
  ]);
});

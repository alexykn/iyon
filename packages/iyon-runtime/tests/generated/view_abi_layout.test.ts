// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = d243e278b8f4640f3ae5de70c311edd1a444f7a8f6359fdf90aea70187aa9951
// generator_blake3 = 96ec2f1ad0ee36f4d1f5352aeac7f6eb649dbfef93482a946523580365c505f9
import { expect, test } from "bun:test";
import manifest from "../../src/tui/generated/view_abi_manifest.json";

test("generated ABI manifest is pinned and ordered", () => {
  expect(manifest.schema_blake3).toBe("d243e278b8f4640f3ae5de70c311edd1a444f7a8f6359fdf90aea70187aa9951");
  expect(manifest.abi.version).toBe(1);
  expect(manifest.functions.map((item) => item.name)).toEqual([
    "runtime_noop",
    "view_render_ref",
    "view_spacer_create",
    "view_text_layout_patch_root",
    "view_common_patch_root",
    "view_axis_create_buffer",
    "view_release_many",
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
    ["runtime_ptr", "u32", "u32", "u32"],
    ["runtime_ptr", "native_ref", "u32", "u32", "u32", "u32"],
    ["runtime_ptr", "native_ref", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "native_ref"],
    ["runtime_ptr", "u32", "u32", "u32", "u32", "pod_slice", "buffer_length", "buffer_used"],
    ["runtime_ptr", "buffer", "buffer_length", "buffer_used"],
  ]);
});

// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = 99cb1472686316689de8d738c78dffa5c60e460d5849a235512a038af55c89e3
// generator_blake3 = 64203215f9f3f54cee942b261ff94b84b6c5440bf1a2e387347674b3df5383dd
import { expect, test } from "bun:test";
import manifest from "../../src/tui/generated/view_abi_manifest.json";

test("generated ABI manifest is pinned and ordered", () => {
  expect(manifest.schema_blake3).toBe("99cb1472686316689de8d738c78dffa5c60e460d5849a235512a038af55c89e3");
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

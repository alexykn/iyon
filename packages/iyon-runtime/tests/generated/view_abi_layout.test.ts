// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = 8cbf2331e1e1c177d2d1ee3e7e98d0e8232324ee3f1cb9039499d4d7f2da58cd
// generator_blake3 = 940295ec578b0cbae431fd55425ab9e421cff5bbcb7250e19aceb66323c817af
import { expect, test } from "bun:test";
import manifest from "../../src/tui/generated/view_abi_manifest.json";

test("generated ABI manifest is pinned and ordered", () => {
  expect(manifest.schema_blake3).toBe("8cbf2331e1e1c177d2d1ee3e7e98d0e8232324ee3f1cb9039499d4d7f2da58cd");
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
    ["runtime_ptr", "u32", "u32", "u32", "u32", "pod_slice", "buffer_length", "u32"],
    ["runtime_ptr", "buffer", "buffer_length", "u32"],
  ]);
});

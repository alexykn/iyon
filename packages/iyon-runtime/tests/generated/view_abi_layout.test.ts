// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = bb04c4b5ded9f38d04dd850fed1d99f76bd781a51ab04f140ab35fa14273bf29
// generator_blake3 = 5134bc9ebe5a949bd99ece560feb766e2612a8d4b222c03f6b928e766f625ca3
import { expect, test } from "bun:test";
import manifest from "../../src/tui/generated/view_abi_manifest.json";

test("generated ABI manifest is pinned and ordered", () => {
  expect(manifest.schema_blake3).toBe("bb04c4b5ded9f38d04dd850fed1d99f76bd781a51ab04f140ab35fa14273bf29");
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

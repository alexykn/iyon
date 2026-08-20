// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = 1ca0fdeba92ffd1a195a4898f5629f1f10f849155f8b8b80b03fe1bd050030a8
// generator_blake3 = 5134bc9ebe5a949bd99ece560feb766e2612a8d4b222c03f6b928e766f625ca3
export type GeneratedAbiBenchmarkCase = {
  name: string;
  family: string;
  hotness: string;
  scalarArgs: number;
  hasBuffer: boolean;
};

export const generatedAbiCases: readonly GeneratedAbiBenchmarkCase[] = [
  { name: "runtime_noop", family: "runtime", hotness: "probe", scalarArgs: 1, hasBuffer: false },
  { name: "view_render_ref", family: "render_ref", hotness: "critical", scalarArgs: 2, hasBuffer: false },
  { name: "view_spacer_create", family: "constructor", hotness: "warm", scalarArgs: 4, hasBuffer: false },
  { name: "view_text_layout_patch_root", family: "scalar_patch", hotness: "critical", scalarArgs: 6, hasBuffer: false },
  { name: "view_common_patch_root", family: "scalar_patch", hotness: "critical", scalarArgs: 14, hasBuffer: false },
  { name: "view_axis_create_buffer", family: "constructor", hotness: "warm", scalarArgs: 4, hasBuffer: true },
  { name: "view_release_many", family: "lifecycle", hotness: "cold", scalarArgs: 2, hasBuffer: true },
];

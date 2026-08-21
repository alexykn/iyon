// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = b6632774c610ea06e51392e4bd1e333cc9cbbb6f39a7ac4c0addff8052b71193
// generator_blake3 = 18452de0513ba234d9b3eab4afe3301ece61e22b53d7d8d242ef1bd7545f6e69
export type GeneratedAbiBenchmarkCase = {
  name: string;
  family: string;
  hotness: string;
  benchmarkRegistration: string;
  scalarArgs: number;
  hasBuffer: boolean;
  maxBufferBytes: number;
  maxInputCount: number;
};

export const generatedAbiCases: readonly GeneratedAbiBenchmarkCase[] = [
  { name: "runtime_noop", family: "runtime", hotness: "probe", benchmarkRegistration: "ffi.noop", scalarArgs: 1, hasBuffer: false, maxBufferBytes: 0, maxInputCount: 0 },
  { name: "view_render_ref", family: "render_ref", hotness: "critical", benchmarkRegistration: "view.render_ref", scalarArgs: 2, hasBuffer: false, maxBufferBytes: 0, maxInputCount: 1 },
  { name: "host_render_ref", family: "render_ref", hotness: "critical", benchmarkRegistration: "view.host_render_ref", scalarArgs: 3, hasBuffer: false, maxBufferBytes: 0, maxInputCount: 1 },
  { name: "view_spacer_create", family: "constructor", hotness: "warm", benchmarkRegistration: "view.spacer_create", scalarArgs: 4, hasBuffer: false, maxBufferBytes: 0, maxInputCount: 1 },
  { name: "view_text_layout_patch_root", family: "scalar_patch", hotness: "critical", benchmarkRegistration: "view.text_layout_patch_root", scalarArgs: 6, hasBuffer: false, maxBufferBytes: 0, maxInputCount: 1 },
  { name: "view_common_patch_root", family: "scalar_patch", hotness: "critical", benchmarkRegistration: "view.common_patch_root", scalarArgs: 14, hasBuffer: false, maxBufferBytes: 0, maxInputCount: 1 },
  { name: "view_axis_create_buffer", family: "constructor", hotness: "warm", benchmarkRegistration: "view.axis_create_buffer", scalarArgs: 6, hasBuffer: true, maxBufferBytes: 4194304, maxInputCount: 524288 },
  { name: "view_release_many", family: "lifecycle", hotness: "cold", benchmarkRegistration: "lifecycle.release_many", scalarArgs: 2, hasBuffer: true, maxBufferBytes: 524288, maxInputCount: 131072 },
  { name: "view_ref_for_node_id", family: "exact_lookup", hotness: "critical", benchmarkRegistration: "view.ref_for_node_id", scalarArgs: 3, hasBuffer: false, maxBufferBytes: 0, maxInputCount: 1 },
  { name: "path_root", family: "path", hotness: "warm", benchmarkRegistration: "path.root", scalarArgs: 1, hasBuffer: false, maxBufferBytes: 0, maxInputCount: 128 },
  { name: "path_child", family: "path", hotness: "warm", benchmarkRegistration: "path.child", scalarArgs: 5, hasBuffer: false, maxBufferBytes: 0, maxInputCount: 128 },
  { name: "view_text_layout_patch_path", family: "path_patch", hotness: "critical", benchmarkRegistration: "view.text_layout_patch_path", scalarArgs: 16, hasBuffer: false, maxBufferBytes: 0, maxInputCount: 4 },
  { name: "view_text_layout_patch_path_d1", family: "path_patch", hotness: "critical", benchmarkRegistration: "view.text_layout_patch_path_d1", scalarArgs: 9, hasBuffer: false, maxBufferBytes: 0, maxInputCount: 1 },
  { name: "view_text_layout_patch_path_d2", family: "path_patch", hotness: "critical", benchmarkRegistration: "view.text_layout_patch_path_d2", scalarArgs: 11, hasBuffer: false, maxBufferBytes: 0, maxInputCount: 2 },
  { name: "view_text_layout_patch_path_d3", family: "path_patch", hotness: "critical", benchmarkRegistration: "view.text_layout_patch_path_d3", scalarArgs: 13, hasBuffer: false, maxBufferBytes: 0, maxInputCount: 3 },
  { name: "view_text_layout_patch_path_d4", family: "path_patch", hotness: "critical", benchmarkRegistration: "view.text_layout_patch_path_d4", scalarArgs: 15, hasBuffer: false, maxBufferBytes: 0, maxInputCount: 4 },
];

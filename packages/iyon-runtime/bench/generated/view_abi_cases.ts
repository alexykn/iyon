// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = d243e278b8f4640f3ae5de70c311edd1a444f7a8f6359fdf90aea70187aa9951
// generator_blake3 = fd3bcd32d6995e625fada939bf2fd398b6dac2ec14400458b75f612cdc4d0d6d
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
  { name: "view_spacer_create", family: "constructor", hotness: "warm", benchmarkRegistration: "view.spacer_create", scalarArgs: 4, hasBuffer: false, maxBufferBytes: 0, maxInputCount: 1 },
  { name: "view_text_layout_patch_root", family: "scalar_patch", hotness: "critical", benchmarkRegistration: "view.text_layout_patch_root", scalarArgs: 6, hasBuffer: false, maxBufferBytes: 0, maxInputCount: 1 },
  { name: "view_common_patch_root", family: "scalar_patch", hotness: "critical", benchmarkRegistration: "view.common_patch_root", scalarArgs: 14, hasBuffer: false, maxBufferBytes: 0, maxInputCount: 1 },
  { name: "view_axis_create_buffer", family: "constructor", hotness: "warm", benchmarkRegistration: "view.axis_create_buffer", scalarArgs: 6, hasBuffer: true, maxBufferBytes: 4194304, maxInputCount: 524288 },
  { name: "view_release_many", family: "lifecycle", hotness: "cold", benchmarkRegistration: "lifecycle.release_many", scalarArgs: 2, hasBuffer: true, maxBufferBytes: 524288, maxInputCount: 131072 },
];

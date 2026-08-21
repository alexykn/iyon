// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = 7fce882a8b31b7dab23c5515ffde2626513fed07f46366e3d9869a966fe1ccb1
// generator_blake3 = 6767bb7dce54c663ecaf7a84446e62ac37ca5f81733789e365918308bcee71b0
import { linkSymbols, type Pointer } from "bun:ffi";
export type NativeAbiConformancePointers = {
  u8_8: Pointer;
  u16_8: Pointer;
  u32_8: Pointer;
  u32_16: Pointer;
  i32_4: Pointer;
  f32_4: Pointer;
  f64_4: Pointer;
  pointer: Pointer;
  buffer: Pointer;
  cstring: Pointer;
};

export function linkViewAbiConformance(abi: NativeAbiConformancePointers) {
  return linkSymbols({
    u8_8: { ptr: abi.u8_8, args: ["u8", "u8", "u8", "u8", "u8", "u8", "u8", "u8"], returns: "u32" },
    u16_8: { ptr: abi.u16_8, args: ["u16", "u16", "u16", "u16", "u16", "u16", "u16", "u16"], returns: "u32" },
    u32_8: { ptr: abi.u32_8, args: ["u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32"], returns: "u32" },
    u32_16: { ptr: abi.u32_16, args: ["u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32"], returns: "u32" },
    i32_4: { ptr: abi.i32_4, args: ["i32", "i32", "i32", "i32"], returns: "i32" },
    f32_4: { ptr: abi.f32_4, args: ["f32", "f32", "f32", "f32"], returns: "f32" },
    f64_4: { ptr: abi.f64_4, args: ["f64", "f64", "f64", "f64"], returns: "f64" },
    pointer: { ptr: abi.pointer, args: ["ptr"], returns: "u32" },
    buffer: { ptr: abi.buffer, args: ["buffer", "buffer_length"], returns: "u32" },
    cstring: { ptr: abi.cstring, args: ["cstring"], returns: "u32" },
  } as const);
}

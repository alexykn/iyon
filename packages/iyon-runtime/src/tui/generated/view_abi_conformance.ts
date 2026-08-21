// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = 823264c7f1539c872782879f296f3782e157960ece5969f64007bb7e5430d801
// generator_blake3 = 9c69e5f6b013b2655aa249b00601622b4d569cb6806fb25863e0d71fe93f53de
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

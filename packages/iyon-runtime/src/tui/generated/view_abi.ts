// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = 99cb1472686316689de8d738c78dffa5c60e460d5849a235512a038af55c89e3
// generator_blake3 = 64203215f9f3f54cee942b261ff94b84b6c5440bf1a2e387347674b3df5383dd
import { linkSymbols, type Pointer } from "bun:ffi";
export type NativeAbiPointers = {
  runtimeNoop: Pointer;
  viewRenderRef: Pointer;
  viewSpacerCreate: Pointer;
  viewTextLayoutPatchRoot: Pointer;
  viewCommonPatchRoot: Pointer;
  viewAxisCreateBuffer: Pointer;
  viewReleaseMany: Pointer;
};

export function linkViewAbi(abi: NativeAbiPointers) {
  return linkSymbols({
    runtimeNoop: { ptr: abi.runtimeNoop, args: ["ptr"], returns: "u32" },
    viewRenderRef: { ptr: abi.viewRenderRef, args: ["ptr", "u32"], returns: "u32" },
    viewSpacerCreate: { ptr: abi.viewSpacerCreate, args: ["ptr", "u32", "u32", "u32"], returns: "u32" },
    viewTextLayoutPatchRoot: { ptr: abi.viewTextLayoutPatchRoot, args: ["ptr", "u32", "u32", "u32", "u32", "u32"], returns: "u32" },
    viewCommonPatchRoot: { ptr: abi.viewCommonPatchRoot, args: ["ptr", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32"], returns: "u32" },
    viewAxisCreateBuffer: { ptr: abi.viewAxisCreateBuffer, args: ["ptr", "u32", "u32", "u32", "u32", "buffer", "buffer_length", "u32"], returns: "u32" },
    viewReleaseMany: { ptr: abi.viewReleaseMany, args: ["ptr", "buffer", "buffer_length", "u32"], returns: "i32" },
  } as const);
}

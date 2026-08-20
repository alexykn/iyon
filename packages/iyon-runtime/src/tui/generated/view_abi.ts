// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = 8cbf2331e1e1c177d2d1ee3e7e98d0e8232324ee3f1cb9039499d4d7f2da58cd
// generator_blake3 = 940295ec578b0cbae431fd55425ab9e421cff5bbcb7250e19aceb66323c817af
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

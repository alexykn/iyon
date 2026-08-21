// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = 6d48b5fb628a89c0b15704063123680036403cf823929071fb92e0af92afe2d9
// generator_blake3 = 18452de0513ba234d9b3eab4afe3301ece61e22b53d7d8d242ef1bd7545f6e69
import { linkSymbols, type Pointer } from "bun:ffi";
export type NativeAbiPointers = {
  runtimeNoop: Pointer;
  viewRenderRef: Pointer;
  hostRenderRef: Pointer;
  viewSpacerCreate: Pointer;
  viewTextLayoutPatchRoot: Pointer;
  viewCommonPatchRoot: Pointer;
  viewAxisCreateBuffer: Pointer;
  viewReleaseMany: Pointer;
  viewRefForNodeId: Pointer;
  pathRoot: Pointer;
  pathChild: Pointer;
  viewTextLayoutPatchPath: Pointer;
  viewTextLayoutPatchPathD1: Pointer;
  viewTextLayoutPatchPathD2: Pointer;
  viewTextLayoutPatchPathD3: Pointer;
  viewTextLayoutPatchPathD4: Pointer;
  editTxnBegin: Pointer;
  editTxnAddTextLayout: Pointer;
  editTxnCommitRender: Pointer;
  editTxnAbort: Pointer;
};

export function linkViewAbi(abi: NativeAbiPointers) {
  return linkSymbols({
    runtimeNoop: { ptr: abi.runtimeNoop, args: ["ptr"], returns: "u32" },
    viewRenderRef: { ptr: abi.viewRenderRef, args: ["ptr", "u32"], returns: "u32" },
    hostRenderRef: { ptr: abi.hostRenderRef, args: ["ptr", "ptr", "u32"], returns: "i32" },
    viewSpacerCreate: { ptr: abi.viewSpacerCreate, args: ["ptr", "u32", "u32", "u32"], returns: "u32" },
    viewTextLayoutPatchRoot: { ptr: abi.viewTextLayoutPatchRoot, args: ["ptr", "u32", "u32", "u32", "u32", "u32"], returns: "u32" },
    viewCommonPatchRoot: { ptr: abi.viewCommonPatchRoot, args: ["ptr", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32"], returns: "u32" },
    viewAxisCreateBuffer: { ptr: abi.viewAxisCreateBuffer, args: ["ptr", "u32", "u32", "u32", "u32", "buffer", "buffer_length", "u32"], returns: "u32" },
    viewReleaseMany: { ptr: abi.viewReleaseMany, args: ["ptr", "buffer", "buffer_length", "u32"], returns: "i32" },
    viewRefForNodeId: { ptr: abi.viewRefForNodeId, args: ["ptr", "u32", "u32"], returns: "u32" },
    pathRoot: { ptr: abi.pathRoot, args: ["ptr"], returns: "u32" },
    pathChild: { ptr: abi.pathChild, args: ["ptr", "u32", "u32", "u32", "u32"], returns: "u32" },
    viewTextLayoutPatchPath: { ptr: abi.viewTextLayoutPatchPath, args: ["ptr", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32"], returns: "u32" },
    viewTextLayoutPatchPathD1: { ptr: abi.viewTextLayoutPatchPathD1, args: ["ptr", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32"], returns: "u32" },
    viewTextLayoutPatchPathD2: { ptr: abi.viewTextLayoutPatchPathD2, args: ["ptr", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32"], returns: "u32" },
    viewTextLayoutPatchPathD3: { ptr: abi.viewTextLayoutPatchPathD3, args: ["ptr", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32"], returns: "u32" },
    viewTextLayoutPatchPathD4: { ptr: abi.viewTextLayoutPatchPathD4, args: ["ptr", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32"], returns: "u32" },
    editTxnBegin: { ptr: abi.editTxnBegin, args: ["ptr", "u32", "u32"], returns: "u32" },
    editTxnAddTextLayout: { ptr: abi.editTxnAddTextLayout, args: ["ptr", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32"], returns: "i32" },
    editTxnCommitRender: { ptr: abi.editTxnCommitRender, args: ["ptr", "ptr", "u32"], returns: "u32" },
    editTxnAbort: { ptr: abi.editTxnAbort, args: ["ptr", "u32"], returns: "i32" },
  } as const);
}

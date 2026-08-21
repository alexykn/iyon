// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = d678d329a5e75554bc9572deb3a4b0dbd95c505cbfc6b1c2de7635483ac81914
// generator_blake3 = 6a3096554d5af17ad3d1aee961024cf2303a623e5ec4a1ecf60275343341dc91
import { linkSymbols, type Pointer } from "bun:ffi";
export type NativeAbiPointers = {
  runtimeNoop: Pointer;
  viewRenderRef: Pointer;
  hostRenderRef: Pointer;
  viewSpacerCreate: Pointer;
  viewTextLayoutPatchRoot: Pointer;
  viewCommonPatchRoot: Pointer;
  viewAxisCreateBuffer: Pointer;
  viewAxisSetChild: Pointer;
  viewAxisSpliceBuffer: Pointer;
  viewGridSetCell: Pointer;
  viewAxisSetChildPath: Pointer;
  viewGridSetCellPath: Pointer;
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
    viewAxisSetChild: { ptr: abi.viewAxisSetChild, args: ["ptr", "u32", "u32", "u32", "u32", "u32", "u32"], returns: "u32" },
    viewAxisSpliceBuffer: { ptr: abi.viewAxisSpliceBuffer, args: ["ptr", "u32", "u32", "u32", "u32", "u32", "buffer", "buffer_length", "u32"], returns: "u32" },
    viewGridSetCell: { ptr: abi.viewGridSetCell, args: ["ptr", "u32", "u32", "u32", "u32", "u32", "u32"], returns: "u32" },
    viewAxisSetChildPath: { ptr: abi.viewAxisSetChildPath, args: ["ptr", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32"], returns: "u32" },
    viewGridSetCellPath: { ptr: abi.viewGridSetCellPath, args: ["ptr", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32"], returns: "u32" },
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

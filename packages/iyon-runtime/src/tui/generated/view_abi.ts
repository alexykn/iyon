// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = 0e958fc44a24679d0503ae6606bcf1529e0b7caf5eed48097ddbe2ade7d60ef6
// generator_blake3 = 20435cb0e211e543dd671e6c86669cf3f205c8e77c5070f47f4d181a4a9d3c71
import { linkSymbols, type Pointer } from "bun:ffi";
export type NativeAbiPointers = {
  runtimeNoop: Pointer;
  viewRenderRef: Pointer;
  hostRenderRef: Pointer;
  viewSpacerCreate: Pointer;
  viewTextLayoutPatchRoot: Pointer;
  viewCommonPatchRoot: Pointer;
  viewAxisCreateBuffer: Pointer;
  viewRowCreate0: Pointer;
  viewRowCreate1: Pointer;
  viewRowCreate2: Pointer;
  viewRowCreate3: Pointer;
  viewRowCreate4: Pointer;
  viewColumnCreate0: Pointer;
  viewColumnCreate1: Pointer;
  viewColumnCreate2: Pointer;
  viewColumnCreate3: Pointer;
  viewColumnCreate4: Pointer;
  axisBuilderBegin: Pointer;
  axisBuilderPush: Pointer;
  axisBuilderFinish: Pointer;
  axisBuilderAbort: Pointer;
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
  styleAtomCreateCstring: Pointer;
  styleCreateBits: Pointer;
  viewTextCreateCstring: Pointer;
  viewTextCreateUtf8: Pointer;
  viewTextCreateCstring2: Pointer;
  viewTextCreateCstring3: Pointer;
  viewTextCreateCstring4: Pointer;
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
    viewRowCreate0: { ptr: abi.viewRowCreate0, args: ["ptr", "u32", "u32", "u32"], returns: "u32" },
    viewRowCreate1: { ptr: abi.viewRowCreate1, args: ["ptr", "u32", "u32", "u32", "u32", "u32"], returns: "u32" },
    viewRowCreate2: { ptr: abi.viewRowCreate2, args: ["ptr", "u32", "u32", "u32", "u32", "u32", "u32", "u32"], returns: "u32" },
    viewRowCreate3: { ptr: abi.viewRowCreate3, args: ["ptr", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32"], returns: "u32" },
    viewRowCreate4: { ptr: abi.viewRowCreate4, args: ["ptr", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32"], returns: "u32" },
    viewColumnCreate0: { ptr: abi.viewColumnCreate0, args: ["ptr", "u32", "u32", "u32"], returns: "u32" },
    viewColumnCreate1: { ptr: abi.viewColumnCreate1, args: ["ptr", "u32", "u32", "u32", "u32", "u32"], returns: "u32" },
    viewColumnCreate2: { ptr: abi.viewColumnCreate2, args: ["ptr", "u32", "u32", "u32", "u32", "u32", "u32", "u32"], returns: "u32" },
    viewColumnCreate3: { ptr: abi.viewColumnCreate3, args: ["ptr", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32"], returns: "u32" },
    viewColumnCreate4: { ptr: abi.viewColumnCreate4, args: ["ptr", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32", "u32"], returns: "u32" },
    axisBuilderBegin: { ptr: abi.axisBuilderBegin, args: ["ptr", "u32", "u32"], returns: "u32" },
    axisBuilderPush: { ptr: abi.axisBuilderPush, args: ["ptr", "u32", "u32", "u32"], returns: "i32" },
    axisBuilderFinish: { ptr: abi.axisBuilderFinish, args: ["ptr", "u32", "u32", "u32", "u32"], returns: "u32" },
    axisBuilderAbort: { ptr: abi.axisBuilderAbort, args: ["ptr", "u32"], returns: "i32" },
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
    styleAtomCreateCstring: { ptr: abi.styleAtomCreateCstring, args: ["ptr", "cstring"], returns: "u32" },
    styleCreateBits: { ptr: abi.styleCreateBits, args: ["ptr", "u32", "u32", "u32", "u32", "u32", "u32"], returns: "u32" },
    viewTextCreateCstring: { ptr: abi.viewTextCreateCstring, args: ["ptr", "u32", "u32", "cstring", "u32", "u32", "u32"], returns: "u32" },
    viewTextCreateUtf8: { ptr: abi.viewTextCreateUtf8, args: ["ptr", "u32", "u32", "buffer", "buffer_length", "u32", "u32", "u32", "u32"], returns: "u32" },
    viewTextCreateCstring2: { ptr: abi.viewTextCreateCstring2, args: ["ptr", "u32", "u32", "cstring", "u32", "cstring", "u32", "u32", "u32"], returns: "u32" },
    viewTextCreateCstring3: { ptr: abi.viewTextCreateCstring3, args: ["ptr", "u32", "u32", "cstring", "u32", "cstring", "u32", "cstring", "u32", "u32", "u32"], returns: "u32" },
    viewTextCreateCstring4: { ptr: abi.viewTextCreateCstring4, args: ["ptr", "u32", "u32", "cstring", "u32", "cstring", "u32", "cstring", "u32", "cstring", "u32", "u32", "u32"], returns: "u32" },
  } as const);
}

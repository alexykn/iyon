// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = 68e52f9913c6c1252f2a061ff4f942c1b32aac35f32bced41e8f9bdc5b2bacb9
// generator_blake3 = 24d34b5e76bb7302928f251bbf11d78e62dfba0dee9cefe44e46082a1aeedc18
import type { Pointer } from "bun:ffi";
import type { linkViewAbi } from "./view_abi";
export type ViewAbiSymbols = ReturnType<typeof linkViewAbi>["symbols"];

const ERROR_BIT = 0x8000_0000;

function checkedRef(result: number): number {
  if (result === 0 || result >= ERROR_BIT) throw new Error(`native ABI status 0x${result.toString(16)}`);
  return result;
}

export function runtimeNoop(symbols: ViewAbiSymbols, runtime: Pointer): number {
  const result = symbols.runtimeNoop(runtime);
  return result;
}

export function viewRenderRef(symbols: ViewAbiSymbols, runtime: Pointer, base: number): number {
  const result = symbols.viewRenderRef(runtime, base);
  return checkedRef(result);
}

export function viewSpacerCreate(symbols: ViewAbiSymbols, runtime: Pointer, node_id_low: number, node_id_high: number, rows: number): number {
  const result = symbols.viewSpacerCreate(runtime, node_id_low, node_id_high, rows);
  return checkedRef(result);
}

export function viewTextLayoutPatchRoot(symbols: ViewAbiSymbols, runtime: Pointer, base: number, node_id_low: number, node_id_high: number, wrap: number, align: number): number {
  const result = symbols.viewTextLayoutPatchRoot(runtime, base, node_id_low, node_id_high, wrap, align);
  return checkedRef(result);
}

export function viewCommonPatchRoot(symbols: ViewAbiSymbols, runtime: Pointer, base: number, node_id_low: number, node_id_high: number, mask: number, padding_tr: number, padding_bl: number, width_rule: number, height_rule: number, min_width: number, max_width: number, min_height: number, max_height: number, decoration_ref: number): number {
  const result = symbols.viewCommonPatchRoot(runtime, base, node_id_low, node_id_high, mask, padding_tr, padding_bl, width_rule, height_rule, min_width, max_width, min_height, max_height, decoration_ref);
  return checkedRef(result);
}

export function viewAxisCreateBuffer(symbols: ViewAbiSymbols, runtime: Pointer, node_id_low: number, node_id_high: number, axis_kind: number, gap: number, children: NodeJS.TypedArray | DataView, used_child_count: number): number {
  const result = symbols.viewAxisCreateBuffer(runtime, node_id_low, node_id_high, axis_kind, gap, children, children, used_child_count);
  return checkedRef(result);
}

export function viewReleaseMany(symbols: ViewAbiSymbols, runtime: Pointer, refs: NodeJS.TypedArray | DataView, used_ref_count: number): number {
  const result = symbols.viewReleaseMany(runtime, refs, refs, used_ref_count);
  return result;
}


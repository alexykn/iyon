/**
 * Cold/new-graph routing thresholds selected by PERF-11.8. Small arities use
 * one generated constructor; larger supported axes use the typed native
 * builder; oversized or unsupported graphs remain on V4/direct fallback.
 */
export const NATIVE_SMALL_AXIS_ARITY_MAX = 4;
export const NATIVE_BUILDER_MAX_CHILDREN = 524_288;
export const NATIVE_COLD_MAX_NODES = 524_288;
export const NATIVE_COLD_MAX_DEPTH = 128;
export const NATIVE_TEXT_MAX_BYTES = 16_777_216;

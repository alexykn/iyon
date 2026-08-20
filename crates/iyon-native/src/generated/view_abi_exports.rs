// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = 8cbf2331e1e1c177d2d1ee3e7e98d0e8232324ee3f1cb9039499d4d7f2da58cd
// generator_blake3 = b62dd2ee81e098b7bab79dfe2cac9c9231d38cb0beaeabd4744b7565c37b8985
// Generated C ABI wrappers. Semantic implementations are supplied by the next tranche.
use super::{NativeViewRuntime, AxisChildInputV1};
pub mod generated_impls {
    use super::{AxisChildInputV1, NativeViewRuntime};
    unsafe extern "Rust" {
        pub fn runtime_noop_impl(runtime: *mut NativeViewRuntime) -> u32;
    }
    unsafe extern "Rust" {
        pub fn view_render_ref_impl(runtime: *mut NativeViewRuntime, base: u32) -> u32;
    }
    unsafe extern "Rust" {
        pub fn view_spacer_create_impl(
            runtime: *mut NativeViewRuntime,
            node_id_low: u32,
            node_id_high: u32,
            rows: u32,
        ) -> u32;
    }
    unsafe extern "Rust" {
        pub fn view_text_layout_patch_root_impl(
            runtime: *mut NativeViewRuntime,
            base: u32,
            node_id_low: u32,
            node_id_high: u32,
            wrap: u32,
            align: u32,
        ) -> u32;
    }
    unsafe extern "Rust" {
        pub fn view_common_patch_root_impl(
            runtime: *mut NativeViewRuntime,
            base: u32,
            node_id_low: u32,
            node_id_high: u32,
            mask: u32,
            padding_tr: u32,
            padding_bl: u32,
            width_rule: u32,
            height_rule: u32,
            min_width: u32,
            max_width: u32,
            min_height: u32,
            max_height: u32,
            decoration_ref: u32,
        ) -> u32;
    }
    unsafe extern "Rust" {
        pub fn view_axis_create_buffer_impl(
            runtime: *mut NativeViewRuntime,
            node_id_low: u32,
            node_id_high: u32,
            axis_kind: u32,
            gap: u32,
            children: *const AxisChildInputV1,
            children_capacity_bytes: usize,
            used_child_count: u32,
        ) -> u32;
    }
    unsafe extern "Rust" {
        pub fn view_release_many_impl(
            runtime: *mut NativeViewRuntime,
            refs: *const u32,
            refs_capacity_bytes: usize,
            used_ref_count: u32,
        ) -> i32;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iyon_runtime_noop_v1(runtime: *mut NativeViewRuntime) -> u32 {
    unsafe { generated_impls::runtime_noop_impl(runtime) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iyon_view_render_ref_v1(
    runtime: *mut NativeViewRuntime,
    base: u32,
) -> u32 {
    unsafe { generated_impls::view_render_ref_impl(runtime, base) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iyon_view_spacer_create_v1(
    runtime: *mut NativeViewRuntime,
    node_id_low: u32,
    node_id_high: u32,
    rows: u32,
) -> u32 {
    unsafe { generated_impls::view_spacer_create_impl(runtime, node_id_low, node_id_high, rows) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iyon_view_text_layout_patch_root_v1(
    runtime: *mut NativeViewRuntime,
    base: u32,
    node_id_low: u32,
    node_id_high: u32,
    wrap: u32,
    align: u32,
) -> u32 {
    unsafe {
        generated_impls::view_text_layout_patch_root_impl(
            runtime,
            base,
            node_id_low,
            node_id_high,
            wrap,
            align,
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iyon_view_common_patch_root_v1(
    runtime: *mut NativeViewRuntime,
    base: u32,
    node_id_low: u32,
    node_id_high: u32,
    mask: u32,
    padding_tr: u32,
    padding_bl: u32,
    width_rule: u32,
    height_rule: u32,
    min_width: u32,
    max_width: u32,
    min_height: u32,
    max_height: u32,
    decoration_ref: u32,
) -> u32 {
    unsafe {
        generated_impls::view_common_patch_root_impl(
            runtime,
            base,
            node_id_low,
            node_id_high,
            mask,
            padding_tr,
            padding_bl,
            width_rule,
            height_rule,
            min_width,
            max_width,
            min_height,
            max_height,
            decoration_ref,
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iyon_view_axis_create_buffer_v1(
    runtime: *mut NativeViewRuntime,
    node_id_low: u32,
    node_id_high: u32,
    axis_kind: u32,
    gap: u32,
    children: *const AxisChildInputV1,
    children_capacity_bytes: usize,
    used_child_count: u32,
) -> u32 {
    unsafe {
        generated_impls::view_axis_create_buffer_impl(
            runtime,
            node_id_low,
            node_id_high,
            axis_kind,
            gap,
            children,
            children_capacity_bytes,
            used_child_count,
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn iyon_view_release_many_v1(
    runtime: *mut NativeViewRuntime,
    refs: *const u32,
    refs_capacity_bytes: usize,
    used_ref_count: u32,
) -> i32 {
    unsafe {
        generated_impls::view_release_many_impl(runtime, refs, refs_capacity_bytes, used_ref_count)
    }
}

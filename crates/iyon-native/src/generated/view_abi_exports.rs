// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = 99cb1472686316689de8d738c78dffa5c60e460d5849a235512a038af55c89e3
// generator_blake3 = 4219366fc3b1474f5656e00c09aedafb88e34234668757d831421864671d1533
// Generated C ABI wrappers. Semantic implementations are supplied by the next tranche.
use super::{NativeViewRuntime, AxisChildInputV1};
pub mod generated_impls {
    use super::{NativeViewRuntime, AxisChildInputV1};
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
#[allow(dead_code)]
fn generated_catch_unwind<T: Copy>(
    work: impl FnOnce() -> Result<T, T>,
    panic_value: T,
) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)) {
        Ok(result) => result.unwrap_or_else(|error| error),
        Err(_) => panic_value,
    }
}
#[allow(dead_code)]
fn generated_nonnull<T: Copy, P>(value: *mut P, error: T) -> Result<*mut P, T> {
    if value.is_null() { Err(error) } else { Ok(value) }
}
#[allow(dead_code)]
fn generated_nonnull_const<T: Copy, P>(
    value: *const P,
    error: T,
) -> Result<*const P, T> {
    if value.is_null() { Err(error) } else { Ok(value) }
}
#[allow(dead_code)]
fn generated_buffer<T: Copy, P>(
    value: *const P,
    capacity_bytes: usize,
    element_size: usize,
    maximum_bytes: u64,
    error: T,
) -> Result<*const P, T> {
    if capacity_bytes as u64 > maximum_bytes || capacity_bytes % element_size != 0
        || (capacity_bytes != 0
            && (value.is_null() || (value as usize) % ::core::mem::align_of::<P>() != 0))
    {
        Err(error)
    } else {
        Ok(value)
    }
}
#[allow(dead_code)]
fn generated_buffer_used<T: Copy>(
    used_count: u32,
    capacity_bytes: usize,
    element_size: usize,
    maximum_count: u32,
    error: T,
) -> Result<u32, T> {
    if used_count > maximum_count
        || (used_count as usize).saturating_mul(element_size) > capacity_bytes
    {
        Err(error)
    } else {
        Ok(used_count)
    }
}
#[allow(dead_code)]
fn generated_native_ref<T: Copy>(value: u32, error: T) -> Result<u32, T> {
    if value == 0 || value >= 0x8000_0000 { Err(error) } else { Ok(value) }
}
#[allow(dead_code)]
fn generated_node_id<T: Copy>(low: u32, high: u32, error: T) -> Result<(u32, u32), T> {
    if high > 0x001f_ffff || (high == 0 && low == 0) {
        Err(error)
    } else {
        Ok((low, high))
    }
}
#[allow(dead_code)]
fn generated_enum<T: Copy>(value: u32, allowed: &[u32], error: T) -> Result<u32, T> {
    if allowed.contains(&value) { Ok(value) } else { Err(error) }
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iyon_runtime_noop_v1(runtime: *mut NativeViewRuntime) -> u32 {
    generated_catch_unwind(
        || {
            (|| -> Result<u32, u32> {
                let runtime = generated_nonnull(runtime, 0x8000_0001u32)?;
                Ok(unsafe { generated_impls::runtime_noop_impl(runtime) })
            })()
        },
        0x8000_00ffu32,
    )
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iyon_view_render_ref_v1(
    runtime: *mut NativeViewRuntime,
    base: u32,
) -> u32 {
    generated_catch_unwind(
        || {
            (|| -> Result<u32, u32> {
                let runtime = generated_nonnull(runtime, 0x8000_0001u32)?;
                let base = generated_native_ref(base, 0x8000_0001u32)?;
                Ok(unsafe { generated_impls::view_render_ref_impl(runtime, base) })
            })()
        },
        0x8000_00ffu32,
    )
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iyon_view_spacer_create_v1(
    runtime: *mut NativeViewRuntime,
    node_id_low: u32,
    node_id_high: u32,
    rows: u32,
) -> u32 {
    generated_catch_unwind(
        || {
            (|| -> Result<u32, u32> {
                let runtime = generated_nonnull(runtime, 0x8000_0001u32)?;
                let (node_id_low, node_id_high) = generated_node_id(
                    node_id_low,
                    node_id_high,
                    0x8000_0001u32,
                )?;
                Ok(unsafe {
                    generated_impls::view_spacer_create_impl(
                        runtime,
                        node_id_low,
                        node_id_high,
                        rows,
                    )
                })
            })()
        },
        0x8000_00ffu32,
    )
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
    generated_catch_unwind(
        || {
            (|| -> Result<u32, u32> {
                let runtime = generated_nonnull(runtime, 0x8000_0001u32)?;
                let base = generated_native_ref(base, 0x8000_0001u32)?;
                let (node_id_low, node_id_high) = generated_node_id(
                    node_id_low,
                    node_id_high,
                    0x8000_0001u32,
                )?;
                let wrap = generated_enum(wrap, &[1, 2, 3], 0x8000_0001u32)?;
                let align = generated_enum(align, &[1, 2, 3], 0x8000_0001u32)?;
                Ok(unsafe {
                    generated_impls::view_text_layout_patch_root_impl(
                        runtime,
                        base,
                        node_id_low,
                        node_id_high,
                        wrap,
                        align,
                    )
                })
            })()
        },
        0x8000_00ffu32,
    )
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
    generated_catch_unwind(
        || {
            (|| -> Result<u32, u32> {
                let runtime = generated_nonnull(runtime, 0x8000_0001u32)?;
                let base = generated_native_ref(base, 0x8000_0001u32)?;
                let (node_id_low, node_id_high) = generated_node_id(
                    node_id_low,
                    node_id_high,
                    0x8000_0001u32,
                )?;
                let decoration_ref = generated_native_ref(
                    decoration_ref,
                    0x8000_0001u32,
                )?;
                Ok(unsafe {
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
                })
            })()
        },
        0x8000_00ffu32,
    )
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
    generated_catch_unwind(
        || {
            (|| -> Result<u32, u32> {
                let runtime = generated_nonnull(runtime, 0x8000_0001u32)?;
                let (node_id_low, node_id_high) = generated_node_id(
                    node_id_low,
                    node_id_high,
                    0x8000_0001u32,
                )?;
                let children = generated_buffer(
                    children,
                    children_capacity_bytes,
                    8,
                    4194304,
                    0x8000_0002u32,
                )?;
                let used_child_count = generated_buffer_used(
                    used_child_count,
                    children_capacity_bytes,
                    8,
                    524288,
                    0x8000_0003u32,
                )?;
                Ok(unsafe {
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
                })
            })()
        },
        0x8000_00ffu32,
    )
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn iyon_view_release_many_v1(
    runtime: *mut NativeViewRuntime,
    refs: *const u32,
    refs_capacity_bytes: usize,
    used_ref_count: u32,
) -> i32 {
    generated_catch_unwind(
        || {
            (|| -> Result<i32, i32> {
                let runtime = generated_nonnull(runtime, -1i32)?;
                let refs = generated_buffer(
                    refs,
                    refs_capacity_bytes,
                    4,
                    524288,
                    -2i32,
                )?;
                let used_ref_count = generated_buffer_used(
                    used_ref_count,
                    refs_capacity_bytes,
                    4,
                    131072,
                    -3i32,
                )?;
                Ok(unsafe {
                    generated_impls::view_release_many_impl(
                        runtime,
                        refs,
                        refs_capacity_bytes,
                        used_ref_count,
                    )
                })
            })()
        },
        -127i32,
    )
}

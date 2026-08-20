// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = 99cb1472686316689de8d738c78dffa5c60e460d5849a235512a038af55c89e3
// generator_blake3 = ebf4697aabdf2e40d1ee0cbe90ead29bd79aa1e767e2c4524510ee2709773623
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

fn generated_catch_unwind<T: Copy>(work: impl FnOnce() -> Result<T, T>, panic_value: T) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)) {
        Ok(result) => result.unwrap_or_else(|error| error),
        Err(_) => panic_value,
    }
}

fn generated_nonnull<T: Copy, P>(value: *mut P, error: T) -> Result<*mut P, T> {
    if value.is_null() {
        Err(error)
    } else {
        Ok(value)
    }
}

fn generated_nonnull_const<T: Copy, P>(value: *const P, error: T) -> Result<*const P, T> {
    if value.is_null() {
        Err(error)
    } else {
        Ok(value)
    }
}

fn generated_native_ref<T: Copy>(value: u32, error: T) -> Result<u32, T> {
    if value == 0 || value >= 0x8000_0000 {
        Err(error)
    } else {
        Ok(value)
    }
}

fn generated_capacity<T: Copy>(value: usize, maximum: u64, error: T) -> Result<usize, T> {
    if value as u64 > maximum {
        Err(error)
    } else {
        Ok(value)
    }
}

fn generated_count<T: Copy>(value: u32, maximum: u32, error: T) -> Result<u32, T> {
    if value > maximum {
        Err(error)
    } else {
        Ok(value)
    }
}

fn generated_enum<T: Copy>(value: u32, allowed: &[u32], error: T) -> Result<u32, T> {
    if allowed.contains(&value) {
        Ok(value)
    } else {
        Err(error)
    }
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
                let decoration_ref = generated_native_ref(decoration_ref, 0x8000_0001u32)?;
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
                let children = generated_nonnull_const(children, 0x8000_0002u32)?;
                let children_capacity_bytes =
                    generated_capacity(children_capacity_bytes, 4194304, 0x8000_0002u32)?;
                let used_child_count = generated_count(used_child_count, 524288, 0x8000_0003u32)?;
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
                let refs = generated_nonnull_const(refs, -2i32)?;
                let refs_capacity_bytes = generated_capacity(refs_capacity_bytes, 524288, -2i32)?;
                let used_ref_count = generated_count(used_ref_count, 131072, -3i32)?;
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

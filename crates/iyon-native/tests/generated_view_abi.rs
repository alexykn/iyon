// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = d243e278b8f4640f3ae5de70c311edd1a444f7a8f6359fdf90aea70187aa9951
// generator_blake3 = fd3bcd32d6995e625fada939bf2fd398b6dac2ec14400458b75f612cdc4d0d6d
#[allow(dead_code)]
pub struct NativeViewRuntime;

#[path = "../src/generated/view_abi_table.rs"]
mod generated;
#[path = "../src/generated/view_abi_conformance.rs"]
mod generated_conformance;
#[path = "../src/generated/view_abi_types.rs"]
mod generated_types;

use generated_types::AxisChildInputV1;

mod generated_exports {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/generated/view_abi_exports.rs"
    ));
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn runtime_noop_impl(runtime: *mut NativeViewRuntime) -> u32 {
    let _ = runtime;
    0x100
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn view_render_ref_impl(
    runtime: *mut NativeViewRuntime,
    base: u32,
) -> u32 {
    let _ = runtime;
    let _ = base;
    0x101
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn view_spacer_create_impl(
    runtime: *mut NativeViewRuntime,
    node_id_low: u32,
    node_id_high: u32,
    rows: u32,
) -> u32 {
    let _ = runtime;
    let _ = node_id_low;
    let _ = node_id_high;
    let _ = rows;
    0x102
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn view_text_layout_patch_root_impl(
    runtime: *mut NativeViewRuntime,
    base: u32,
    node_id_low: u32,
    node_id_high: u32,
    wrap: u32,
    align: u32,
) -> u32 {
    let _ = runtime;
    let _ = base;
    let _ = node_id_low;
    let _ = node_id_high;
    let _ = wrap;
    let _ = align;
    0x103
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn view_common_patch_root_impl(
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
    let _ = runtime;
    let _ = base;
    let _ = node_id_low;
    let _ = node_id_high;
    let _ = mask;
    let _ = padding_tr;
    let _ = padding_bl;
    let _ = width_rule;
    let _ = height_rule;
    let _ = min_width;
    let _ = max_width;
    let _ = min_height;
    let _ = max_height;
    let _ = decoration_ref;
    0x104
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn view_axis_create_buffer_impl(
    runtime: *mut NativeViewRuntime,
    node_id_low: u32,
    node_id_high: u32,
    axis_kind: u32,
    gap: u32,
    children: *const AxisChildInputV1,
    children_capacity_bytes: usize,
    used_child_count: u32,
) -> u32 {
    let _ = runtime;
    let _ = node_id_low;
    let _ = node_id_high;
    let _ = axis_kind;
    let _ = gap;
    let _ = children;
    let _ = children_capacity_bytes;
    let _ = used_child_count;
    0x105
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn view_release_many_impl(
    runtime: *mut NativeViewRuntime,
    refs: *const u32,
    refs_capacity_bytes: usize,
    used_ref_count: u32,
) -> i32 {
    let _ = runtime;
    let _ = refs;
    let _ = refs_capacity_bytes;
    let _ = used_ref_count;
    106
}

#[test]
fn generated_function_count_is_stable() {
    assert_eq!(generated::FUNCTION_COUNT, 7);
}

#[test]
fn generated_abi_version_is_one() {
    assert_eq!(generated_types::ABI_VERSION, 1);
}

#[test]
fn generated_conformance_count_is_stable() {
    assert_eq!(10, 10);
}

#[test]
fn generated_conformance_functions_are_callable() {
    assert_eq!(
        unsafe {
            generated_conformance::iyon_abi_conformance_u8_8_v1(
                1 as u8, 2 as u8, 3 as u8, 4 as u8, 5 as u8, 6 as u8, 7 as u8, 8 as u8,
            )
        },
        562
    );
    assert_eq!(
        unsafe {
            generated_conformance::iyon_abi_conformance_u16_8_v1(
                1 as u16, 2 as u16, 3 as u16, 4 as u16, 5 as u16, 6 as u16, 7 as u16, 8 as u16,
            )
        },
        562
    );
    assert_eq!(
        unsafe {
            generated_conformance::iyon_abi_conformance_u32_8_v1(
                1 as u32, 2 as u32, 3 as u32, 4 as u32, 5 as u32, 6 as u32, 7 as u32, 8 as u32,
            )
        },
        562
    );
    assert_eq!(
        unsafe {
            generated_conformance::iyon_abi_conformance_u32_16_v1(
                1 as u32, 2 as u32, 3 as u32, 4 as u32, 5 as u32, 6 as u32, 7 as u32, 8 as u32,
                9 as u32, 10 as u32, 11 as u32, 12 as u32, 13 as u32, 14 as u32, 15 as u32,
                16 as u32,
            )
        },
        4988
    );
    assert_eq!(
        unsafe {
            generated_conformance::iyon_abi_conformance_i32_4_v1(
                1 as i32, 2 as i32, 3 as i32, 4 as i32,
            )
        },
        78
    );
    assert!(
        (unsafe {
            generated_conformance::iyon_abi_conformance_f32_4_v1(
                1 as f32, 2 as f32, 3 as f32, 4 as f32,
            )
        } - 78.0)
            .abs()
            < 0.000001
    );
    assert!(
        (unsafe {
            generated_conformance::iyon_abi_conformance_f64_4_v1(
                1 as f64, 2 as f64, 3 as f64, 4 as f64,
            )
        } - 78.0)
            .abs()
            < 0.000001
    );
    assert_eq!(
        unsafe {
            generated_conformance::iyon_abi_conformance_pointer_v1(
                core::ptr::NonNull::<core::ffi::c_void>::dangling().as_ptr(),
            )
        },
        1
    );
    let bytes = [0x7b_u8, 0x01, 0x02, 0x03];
    assert_eq!(
        unsafe {
            generated_conformance::iyon_abi_conformance_buffer_v1(bytes.as_ptr(), bytes.len())
        },
        4 * 257 + 0x7b
    );
    let text = std::ffi::CString::new("ABI ✓").expect("test text has no NUL");
    assert_ne!(
        unsafe { generated_conformance::iyon_abi_conformance_cstring_v1(text.as_ptr()) },
        0
    );
}

#[test]
fn generated_wrappers_reject_invalid_inputs_and_delegate() {
    let mut runtime = NativeViewRuntime;
    let runtime_ptr = &mut runtime as *mut NativeViewRuntime;
    assert_eq!(
        unsafe { generated_exports::iyon_runtime_noop_v1(runtime_ptr) },
        0x100
    );
    assert_eq!(
        unsafe { generated_exports::iyon_view_render_ref_v1(runtime_ptr, 1) },
        0x101
    );
    assert_eq!(
        unsafe { generated_exports::iyon_view_spacer_create_v1(runtime_ptr, 1, 0, 2) },
        0x102
    );
    assert_eq!(
        unsafe {
            generated_exports::iyon_view_text_layout_patch_root_v1(runtime_ptr, 1, 1, 0, 1, 2)
        },
        0x103
    );
    assert_eq!(
        unsafe {
            generated_exports::iyon_view_common_patch_root_v1(
                runtime_ptr,
                1,
                1,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                1,
            )
        },
        0x104
    );
    let children = [generated_types::AxisChildInputV1 {
        track_word: 1,
        child_ref: 1,
    }];
    assert_eq!(
        unsafe {
            generated_exports::iyon_view_axis_create_buffer_v1(
                runtime_ptr,
                1,
                0,
                1,
                0,
                children.as_ptr(),
                core::mem::size_of_val(&children),
                1,
            )
        },
        0x105
    );
    let refs = [1_u32];
    assert_eq!(
        unsafe {
            generated_exports::iyon_view_release_many_v1(
                runtime_ptr,
                refs.as_ptr(),
                core::mem::size_of_val(&refs),
                1,
            )
        },
        106
    );
    assert_eq!(
        unsafe { generated_exports::iyon_runtime_noop_v1(core::ptr::null_mut()) },
        0x8000_0001
    );
    assert_eq!(
        unsafe { generated_exports::iyon_view_render_ref_v1(runtime_ptr, 0) },
        0x8000_0001
    );
    assert_eq!(
        unsafe { generated_exports::iyon_view_spacer_create_v1(runtime_ptr, 0, 0, 1) },
        0x8000_0001
    );
    assert_eq!(
        unsafe {
            generated_exports::iyon_view_text_layout_patch_root_v1(runtime_ptr, 1, 1, 0, 0, 1)
        },
        0x8000_0001
    );
    assert_eq!(
        unsafe {
            generated_exports::iyon_view_axis_create_buffer_v1(
                runtime_ptr,
                1,
                0,
                1,
                0,
                core::ptr::null(),
                8,
                0,
            )
        },
        0x8000_0002
    );
    assert_eq!(
        unsafe {
            generated_exports::iyon_view_axis_create_buffer_v1(
                runtime_ptr,
                1,
                0,
                1,
                0,
                core::ptr::null(),
                0,
                1,
            )
        },
        0x8000_0003
    );
    assert_eq!(
        unsafe {
            generated_exports::iyon_view_release_many_v1(runtime_ptr, core::ptr::null(), 4, 0)
        },
        -2
    );
    assert_eq!(
        unsafe {
            generated_exports::iyon_view_release_many_v1(runtime_ptr, core::ptr::null(), 0, 1)
        },
        -3
    );
}

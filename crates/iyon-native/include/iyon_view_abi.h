/* DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml. */
/* schema_blake3 = 7fce882a8b31b7dab23c5515ffde2626513fed07f46366e3d9869a966fe1ccb1 */
/* generator_blake3 = 6767bb7dce54c663ecaf7a84446e62ac37ca5f81733789e365918308bcee71b0 */
#ifndef IYON_VIEW_ABI_H
#define IYON_VIEW_ABI_H

#include <stddef.h>
#include <stdint.h>

#define IYON_VIEW_ABI_NAME "iyon_tui_view"
#define IYON_VIEW_ABI_VERSION 1
#define IYON_VIEW_SEMANTIC_SCHEMA_VERSION 1
#define IYON_VIEW_MINIMUM_BUN "1.4.0"
#define IYON_VIEW_QUALIFIED_BUN "1.4.0"
#define IYON_VIEW_RESULT_ERROR_BIT UINT32_C(0x80000000)

typedef struct NativeViewRuntime NativeViewRuntime;
typedef struct NativeHost NativeHost;
typedef struct AxisChildInputV1 {
    uint32_t track_word;
    uint32_t child_ref;
} AxisChildInputV1;

typedef enum WrapMode {
    WrapMode_WordThenGrapheme = UINT32_C(1),
    WrapMode_Grapheme = UINT32_C(2),
    WrapMode_NoWrap = UINT32_C(3),
} WrapMode;

typedef enum HorizontalAlign {
    HorizontalAlign_Start = UINT32_C(1),
    HorizontalAlign_Center = UINT32_C(2),
    HorizontalAlign_End = UINT32_C(3),
} HorizontalAlign;

uint32_t iyon_runtime_noop_v1(NativeViewRuntime * runtime);

uint32_t iyon_view_render_ref_v1(NativeViewRuntime * runtime, uint32_t base);

uint32_t iyon_view_spacer_create_v1(NativeViewRuntime * runtime, uint32_t node_id_low, uint32_t node_id_high, uint32_t rows);

uint32_t iyon_view_text_layout_patch_root_v1(NativeViewRuntime * runtime, uint32_t base, uint32_t node_id_low, uint32_t node_id_high, uint32_t wrap, uint32_t align);

uint32_t iyon_view_common_patch_root_v1(NativeViewRuntime * runtime, uint32_t base, uint32_t node_id_low, uint32_t node_id_high, uint32_t mask, uint32_t padding_tr, uint32_t padding_bl, uint32_t width_rule, uint32_t height_rule, uint32_t min_width, uint32_t max_width, uint32_t min_height, uint32_t max_height, uint32_t decoration_ref);

uint32_t iyon_view_axis_create_buffer_v1(NativeViewRuntime * runtime, uint32_t node_id_low, uint32_t node_id_high, uint32_t axis_kind, uint32_t gap, const AxisChildInputV1 * children, size_t children_capacity_bytes, uint32_t used_child_count);

int32_t iyon_view_release_many_v1(NativeViewRuntime * runtime, const uint32_t * refs, size_t refs_capacity_bytes, uint32_t used_ref_count);

uint32_t iyon_view_ref_for_node_id_v1(NativeViewRuntime * runtime, uint32_t node_id_low, uint32_t node_id_high);

uint32_t iyon_abi_conformance_u8_8_v1(uint8_t a0, uint8_t a1, uint8_t a2, uint8_t a3, uint8_t a4, uint8_t a5, uint8_t a6, uint8_t a7);

uint32_t iyon_abi_conformance_u16_8_v1(uint16_t a0, uint16_t a1, uint16_t a2, uint16_t a3, uint16_t a4, uint16_t a5, uint16_t a6, uint16_t a7);

uint32_t iyon_abi_conformance_u32_8_v1(uint32_t a0, uint32_t a1, uint32_t a2, uint32_t a3, uint32_t a4, uint32_t a5, uint32_t a6, uint32_t a7);

uint32_t iyon_abi_conformance_u32_16_v1(uint32_t a0, uint32_t a1, uint32_t a2, uint32_t a3, uint32_t a4, uint32_t a5, uint32_t a6, uint32_t a7, uint32_t a8, uint32_t a9, uint32_t a10, uint32_t a11, uint32_t a12, uint32_t a13, uint32_t a14, uint32_t a15);

int32_t iyon_abi_conformance_i32_4_v1(int32_t a0, int32_t a1, int32_t a2, int32_t a3);

float iyon_abi_conformance_f32_4_v1(float a0, float a1, float a2, float a3);

double iyon_abi_conformance_f64_4_v1(double a0, double a1, double a2, double a3);

uint32_t iyon_abi_conformance_pointer_v1(void * a0);

uint32_t iyon_abi_conformance_buffer_v1(const uint8_t * a0, size_t a1);

uint32_t iyon_abi_conformance_cstring_v1(const char * a0);

#endif /* IYON_VIEW_ABI_H */

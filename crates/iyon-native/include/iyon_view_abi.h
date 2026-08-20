/* DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml. */
/* schema_blake3 = 1ca0fdeba92ffd1a195a4898f5629f1f10f849155f8b8b80b03fe1bd050030a8 */
/* generator_blake3 = 5134bc9ebe5a949bd99ece560feb766e2612a8d4b222c03f6b928e766f625ca3 */
#ifndef IYON_VIEW_ABI_H
#define IYON_VIEW_ABI_H

#include <stddef.h>
#include <stdint.h>

#define IYON_VIEW_ABI_NAME "iyon_tui_view"
#define IYON_VIEW_ABI_VERSION 1
#define IYON_VIEW_SEMANTIC_SCHEMA_VERSION 1
#define IYON_VIEW_RESULT_ERROR_BIT UINT32_C(0x80000000)

typedef struct NativeViewRuntime NativeViewRuntime;
typedef struct NativeHost NativeHost;
typedef struct AxisChildInputV1 { uint32_t track_word; uint32_t child_ref; } AxisChildInputV1;

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

uint32_t iyon_view_axis_create_buffer_v1(NativeViewRuntime * runtime, uint32_t node_id_low, uint32_t node_id_high, const AxisChildInputV1 * children, size_t children_capacity_bytes, uint32_t used_child_count);

int32_t iyon_view_release_many_v1(NativeViewRuntime * runtime, const uint32_t * refs, size_t refs_capacity_bytes, uint32_t used_ref_count);

#endif /* IYON_VIEW_ABI_H */

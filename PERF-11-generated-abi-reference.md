<!-- DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml. schema_blake3 = d243e278b8f4640f3ae5de70c311edd1a444f7a8f6359fdf90aea70187aa9951; generator_blake3 = fd3bcd32d6995e625fada939bf2fd398b6dac2ec14400458b75f612cdc4d0d6d -->

# PERF-11 generated ABI reference

> This file is generated. Do not edit it directly.

- Schema BLAKE3: `d243e278b8f4640f3ae5de70c311edd1a444f7a8f6359fdf90aea70187aa9951`
- Generator BLAKE3: `fd3bcd32d6995e625fada939bf2fd398b6dac2ec14400458b75f612cdc4d0d6d`
- ABI: `iyon_tui_view` v1
- Semantic schema: v1
- Minimum Bun: `1.4.0`
- Qualified Bun: `1.4.0`
## Handles

| Name | Rust | TypeScript | Lifetime | Kind |
|---|---|---|---|---|
| `RuntimePtr` | `*mut NativeViewRuntime` | `Pointer` | `environment` | `-` |
| `HostPtr` | `*mut NativeHost` | `Pointer` | `host` | `-` |
| `ViewRef` | `u32` | `number` | `runtime` | `view` |
| `PathRef` | `u32` | `number` | `runtime` | `path` |

## POD buffers

| Name | Repr | Size | Align |
|---|---|---:|---:|
| `AxisChildInputV1` | `C` | 8 | 4 |

## Enums

### `WrapMode`

| Value | Bridge key |
|---|---|
| `WordThenGrapheme` | `wrapWordThenGrapheme` |
| `Grapheme` | `wrapGrapheme` |
| `NoWrap` | `wrapNoWrap` |

### `HorizontalAlign`

| Value | Bridge key |
|---|---|
| `Start` | `horizontalStart` |
| `Center` | `horizontalCenter` |
| `End` | `horizontalEnd` |

## Functions

| Name | Family | Hotness | Return | Fallback | Thread | Allocates | Host mutation |
|---|---|---|---|---|---|---|---|
| `runtime_noop` | `runtime` | `probe` | `u32` | `none` | `owner_thread` | `false` | `false` |
| `view_render_ref` | `render_ref` | `critical` | `ViewRefResult` | `v4` | `owner_thread` | `false` | `false` |
| `view_spacer_create` | `constructor` | `warm` | `ViewRefResult` | `v4` | `owner_thread` | `true` | `false` |
| `view_text_layout_patch_root` | `scalar_patch` | `critical` | `ViewRefResult` | `v4` | `owner_thread` | `true` | `false` |
| `view_common_patch_root` | `scalar_patch` | `critical` | `ViewRefResult` | `v4` | `owner_thread` | `true` | `false` |
| `view_axis_create_buffer` | `constructor` | `warm` | `ViewRefResult` | `v4` | `owner_thread` | `true` | `false` |
| `view_release_many` | `lifecycle` | `cold` | `i32` | `none` | `owner_thread` | `false` | `false` |

## ABI conformance fixtures

| Name | Return | Operation | Arguments |
|---|---|---|---|
| `u8_8` | `u32` | `position_weighted_sum` | `u8, u8, u8, u8, u8, u8, u8, u8` |
| `u16_8` | `u32` | `position_weighted_sum` | `u16, u16, u16, u16, u16, u16, u16, u16` |
| `u32_8` | `u32` | `position_weighted_sum` | `u32, u32, u32, u32, u32, u32, u32, u32` |
| `u32_16` | `u32` | `position_weighted_sum` | `u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32, u32` |
| `i32_4` | `i32` | `position_weighted_sum` | `i32, i32, i32, i32` |
| `f32_4` | `f32` | `position_weighted_sum` | `f32, f32, f32, f32` |
| `f64_4` | `f64` | `position_weighted_sum` | `f64, f64, f64, f64` |
| `pointer` | `u32` | `pointer_probe` | `ptr` |
| `buffer` | `u32` | `buffer_probe` | `buffer, buffer_length` |
| `cstring` | `u32` | `cstring_hash` | `cstring` |


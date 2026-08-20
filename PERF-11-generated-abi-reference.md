<!-- DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml. schema_blake3 = 68e52f9913c6c1252f2a061ff4f942c1b32aac35f32bced41e8f9bdc5b2bacb9; generator_blake3 = 24d34b5e76bb7302928f251bbf11d78e62dfba0dee9cefe44e46082a1aeedc18 -->

# PERF-11 generated ABI reference

> This file is generated. Do not edit it directly.

- Schema BLAKE3: `68e52f9913c6c1252f2a061ff4f942c1b32aac35f32bced41e8f9bdc5b2bacb9`
- Generator BLAKE3: `24d34b5e76bb7302928f251bbf11d78e62dfba0dee9cefe44e46082a1aeedc18`
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


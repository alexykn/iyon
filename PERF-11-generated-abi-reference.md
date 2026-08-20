<!-- DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml. schema_blake3 = 1ca0fdeba92ffd1a195a4898f5629f1f10f849155f8b8b80b03fe1bd050030a8; generator_blake3 = 5134bc9ebe5a949bd99ece560feb766e2612a8d4b222c03f6b928e766f625ca3 -->

# PERF-11 generated ABI reference

> This file is generated. Do not edit it directly.

- Schema BLAKE3: `1ca0fdeba92ffd1a195a4898f5629f1f10f849155f8b8b80b03fe1bd050030a8`
- Generator BLAKE3: `5134bc9ebe5a949bd99ece560feb766e2612a8d4b222c03f6b928e766f625ca3`
- ABI: `iyon_tui_view` v1
- Semantic schema: v1
- Minimum Bun: `1.4.0`

## Handles

| Name | Rust | TypeScript | Lifetime | Kind |
|---|---|---|---|---|
| `RuntimePtr` | `*mut NativeViewRuntime` | `Pointer` | `environment` | `-` |
| `HostPtr` | `*mut NativeHost` | `Pointer` | `host` | `-` |
| `ViewRef` | `u32` | `number` | `runtime` | `view` |
| `PathRef` | `u32` | `number` | `runtime` | `path` |

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

| Name | Family | Hotness | Return | Fallback |
|---|---|---|---|---|
| `runtime_noop` | `runtime` | `probe` | `u32` | `none` |
| `view_render_ref` | `render_ref` | `critical` | `ViewRefResult` | `v4` |
| `view_spacer_create` | `constructor` | `warm` | `ViewRefResult` | `v4` |
| `view_text_layout_patch_root` | `scalar_patch` | `critical` | `ViewRefResult` | `v4` |
| `view_common_patch_root` | `scalar_patch` | `critical` | `ViewRefResult` | `v4` |
| `view_axis_create_buffer` | `constructor` | `warm` | `ViewRefResult` | `v4` |
| `view_release_many` | `lifecycle` | `cold` | `i32` | `none` |


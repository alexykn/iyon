// DO NOT EDIT. Generated from tools/tui-abi/view_abi.toml.
// schema_blake3 = 1ca0fdeba92ffd1a195a4898f5629f1f10f849155f8b8b80b03fe1bd050030a8
// generator_blake3 = 5134bc9ebe5a949bd99ece560feb766e2612a8d4b222c03f6b928e766f625ca3
#[derive(Clone, Copy, Debug)]
pub struct FunctionDescriptor {
    pub name: &'static str,
    pub symbol: &'static str,
    pub family: &'static str,
    pub hotness: &'static str,
    pub fallback: &'static str,
}

pub static FUNCTIONS: &[FunctionDescriptor] = &[
    FunctionDescriptor {
        name: "runtime_noop",
        symbol: "iyon_runtime_noop_v1",
        family: "runtime",
        hotness: "probe",
        fallback: "none",
    },
    FunctionDescriptor {
        name: "view_render_ref",
        symbol: "iyon_view_render_ref_v1",
        family: "render_ref",
        hotness: "critical",
        fallback: "v4",
    },
    FunctionDescriptor {
        name: "view_spacer_create",
        symbol: "iyon_view_spacer_create_v1",
        family: "constructor",
        hotness: "warm",
        fallback: "v4",
    },
    FunctionDescriptor {
        name: "view_text_layout_patch_root",
        symbol: "iyon_view_text_layout_patch_root_v1",
        family: "scalar_patch",
        hotness: "critical",
        fallback: "v4",
    },
    FunctionDescriptor {
        name: "view_common_patch_root",
        symbol: "iyon_view_common_patch_root_v1",
        family: "scalar_patch",
        hotness: "critical",
        fallback: "v4",
    },
    FunctionDescriptor {
        name: "view_axis_create_buffer",
        symbol: "iyon_view_axis_create_buffer_v1",
        family: "constructor",
        hotness: "warm",
        fallback: "v4",
    },
    FunctionDescriptor {
        name: "view_release_many",
        symbol: "iyon_view_release_many_v1",
        family: "lifecycle",
        hotness: "cold",
        fallback: "none",
    },
];

pub const FUNCTION_COUNT: usize = FUNCTIONS.len();

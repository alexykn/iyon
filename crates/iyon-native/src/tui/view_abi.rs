use super::{ViewBridgeCache, view_bridge_cache_for_env};
use crate::NativeError;
use iyon_tui::{HorizontalAlign, Insets, View, WrapMode};
use napi::Env;
use napi_derive::napi;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::ThreadId;

#[path = "../generated/view_abi_types.rs"]
mod generated_types;
pub use generated_types::AxisChildInputV1;

mod generated_exports {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/generated/view_abi_exports.rs"
    ));
}

#[path = "../generated/view_abi_table.rs"]
mod generated_table;

const ABI_MAGIC: u32 = 1_230_589_774;
const ABI_VERSION: u32 = 1;
const SEMANTIC_VERSION: u32 = 1;
const FAST_INVALID: u32 = 0x8000_0001;
const FAST_CACHE_MISS: u32 = 0x8000_0004;
const FAST_FALLBACK: u32 = 0x8000_0005;
const FAST_INTERNAL: u32 = 0x8000_00ff;

const PATCH_PADDING: u32 = 4;
const PATCH_WIDTH: u32 = 8;
const PATCH_HEIGHT: u32 = 16;
const PATCH_MIN_WIDTH: u32 = 32;
const PATCH_MAX_WIDTH: u32 = 64;
const PATCH_MIN_HEIGHT: u32 = 128;
const PATCH_MAX_HEIGHT: u32 = 256;
const PATCH_MASK: u32 = PATCH_PADDING
    | PATCH_WIDTH
    | PATCH_HEIGHT
    | PATCH_MIN_WIDTH
    | PATCH_MAX_WIDTH
    | PATCH_MIN_HEIGHT
    | PATCH_MAX_HEIGHT;

struct NativeViewSlot {
    node_id: u64,
    weak: iyon_tui::WeakView,
    leased: Option<View>,
    js_lease_count: u32,
}

#[repr(C)]
pub struct NativeViewRuntime {
    pub magic: u32,
    pub abi_version: u32,
    pub semantic_version: u32,
    pub alive: AtomicU32,
    owner_thread: ThreadId,
    cache: Arc<Mutex<ViewBridgeCache>>,
    slots: HashMap<u32, NativeViewSlot>,
    node_refs: HashMap<u64, u32>,
    next_native_ref: u32,
    generation: u32,
}

impl NativeViewRuntime {
    fn new(cache: Arc<Mutex<ViewBridgeCache>>) -> Self {
        Self {
            magic: ABI_MAGIC,
            abi_version: ABI_VERSION,
            semantic_version: SEMANTIC_VERSION,
            alive: AtomicU32::new(1),
            owner_thread: std::thread::current().id(),
            cache,
            slots: HashMap::new(),
            node_refs: HashMap::new(),
            next_native_ref: 1,
            generation: 1,
        }
    }

    fn valid_on_owner_thread(&self) -> bool {
        self.magic == ABI_MAGIC
            && self.abi_version == ABI_VERSION
            && self.semantic_version == SEMANTIC_VERSION
            && self.alive.load(Ordering::Acquire) != 0
            && self.owner_thread == std::thread::current().id()
    }

    fn allocate_ref(&mut self) -> Option<u32> {
        while self.next_native_ref < 0x8000_0000 {
            let candidate = self.next_native_ref;
            self.next_native_ref = self.next_native_ref.wrapping_add(1);
            if candidate != 0 && !self.slots.contains_key(&candidate) {
                return Some(candidate);
            }
        }
        None
    }

    fn ensure_lease(&mut self, reference: u32, view: View) -> Result<(), u32> {
        let Some(slot) = self.slots.get_mut(&reference) else {
            return Err(FAST_CACHE_MISS);
        };
        if slot.js_lease_count == 0 {
            slot.leased = Some(view);
            slot.js_lease_count = 1;
        } else if slot.leased.is_none() {
            slot.leased = Some(view);
        }
        Ok(())
    }

    fn resolve_ref(&mut self, reference: u32) -> Result<View, u32> {
        let Some(slot) = self.slots.get_mut(&reference) else {
            return Err(FAST_CACHE_MISS);
        };
        if let Some(view) = slot.leased.clone() {
            return Ok(view);
        }
        let Some(view) = slot.weak.upgrade() else {
            let node_id = slot.node_id;
            self.node_refs.remove(&node_id);
            self.slots.remove(&reference);
            return Err(FAST_CACHE_MISS);
        };
        Ok(view)
    }

    fn publish(&mut self, node_id: u64, view: View) -> Result<u32, u32> {
        if node_id == 0 {
            return Err(FAST_INVALID);
        }
        if let Some(reference) = self.node_refs.get(&node_id).copied() {
            match self.resolve_ref(reference) {
                Ok(existing) if existing == view => {
                    self.ensure_lease(reference, existing)?;
                    return Ok(reference);
                }
                Ok(_) => return Err(FAST_INVALID),
                Err(FAST_CACHE_MISS) => {
                    self.node_refs.remove(&node_id);
                }
                Err(error) => return Err(error),
            }
        }

        let reference = self.allocate_ref().ok_or(FAST_FALLBACK)?;
        let weak = view.downgrade();
        {
            let mut cache = self.cache.lock().map_err(|_| FAST_INTERNAL)?;
            if let Some(existing) = cache
                .nodes
                .get(&node_id)
                .and_then(iyon_tui::WeakView::upgrade)
            {
                if existing != view {
                    return Err(FAST_INVALID);
                }
            }
            cache.nodes.insert(node_id, weak.clone());
        }
        self.node_refs.insert(node_id, reference);
        self.slots.insert(
            reference,
            NativeViewSlot {
                node_id,
                weak,
                leased: Some(view),
                js_lease_count: 1,
            },
        );
        Ok(reference)
    }

    fn ref_for_node_id(&mut self, node_id: u64) -> Result<u32, u32> {
        if node_id == 0 {
            return Err(FAST_INVALID);
        }
        if let Some(reference) = self.node_refs.get(&node_id).copied() {
            match self.resolve_ref(reference) {
                Ok(view) => {
                    self.ensure_lease(reference, view)?;
                    return Ok(reference);
                }
                Err(_) => {
                    self.node_refs.remove(&node_id);
                }
            }
        }
        let view = {
            let mut cache = self.cache.lock().map_err(|_| FAST_INTERNAL)?;
            let Some(weak) = cache.nodes.get(&node_id).cloned() else {
                return Err(FAST_CACHE_MISS);
            };
            let Some(view) = weak.upgrade() else {
                cache.nodes.remove(&node_id);
                return Err(FAST_CACHE_MISS);
            };
            view
        };
        self.publish(node_id, view)
    }

    fn release_many(&mut self, refs: *const u32, used_count: u32) -> Result<i32, i32> {
        for index in 0..used_count as usize {
            let reference = unsafe { refs.add(index).read() };
            let remove_slot = self
                .slots
                .get_mut(&reference)
                .map(|slot| {
                    slot.js_lease_count = slot.js_lease_count.saturating_sub(1);
                    if slot.js_lease_count == 0 {
                        slot.leased = None;
                    }
                    slot.js_lease_count == 0 && slot.weak.upgrade().is_none()
                })
                .unwrap_or(false);
            if remove_slot {
                if let Some(slot) = self.slots.remove(&reference) {
                    if self.node_refs.get(&slot.node_id) == Some(&reference) {
                        self.node_refs.remove(&slot.node_id);
                    }
                }
            }
        }
        Ok(used_count as i32)
    }
}

static RUNTIME_POINTERS: OnceLock<Mutex<HashMap<usize, usize>>> = OnceLock::new();

fn runtime_pointers() -> &'static Mutex<HashMap<usize, usize>> {
    RUNTIME_POINTERS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn runtime_for_env(env: &Env) -> napi::Result<*mut NativeViewRuntime> {
    let env_key = env.raw() as usize;
    let mut pointers = runtime_pointers()
        .lock()
        .map_err(|_| NativeError::internal("native View ABI runtime registry is poisoned"))?;
    if let Some(pointer) = pointers.get(&env_key).copied() {
        return Ok(pointer as *mut NativeViewRuntime);
    }
    let cache = view_bridge_cache_for_env(env)?;
    let runtime = Box::new(NativeViewRuntime::new(cache));
    let pointer = Box::into_raw(runtime);
    pointers.insert(env_key, pointer as usize);
    let cleanup_pointer = pointer as usize;
    if let Err(error) = env.add_env_cleanup_hook(cleanup_pointer, |cleanup_pointer| {
        if let Some(registry) = RUNTIME_POINTERS.get()
            && let Ok(mut pointers) = registry.lock()
        {
            pointers.retain(|_, pointer| *pointer != cleanup_pointer);
        }
        unsafe {
            let runtime = &*(cleanup_pointer as *const NativeViewRuntime);
            runtime.alive.store(0, Ordering::Release);
            drop(Box::from_raw(cleanup_pointer as *mut NativeViewRuntime));
        }
    }) {
        pointers.remove(&env_key);
        unsafe {
            drop(Box::from_raw(pointer));
        }
        return Err(error);
    }
    Ok(pointer)
}

#[napi(js_name = "tuiViewAbiBootstrap")]
pub fn bootstrap(env: Env) -> napi::Result<Value> {
    let runtime = runtime_for_env(&env)?;
    Ok(serde_json::json!({
        "runtime_ptr": runtime as usize as u64,
        "abi_name": generated_types::ABI_NAME,
        "abi_version": generated_types::ABI_VERSION,
        "semantic_version": generated_types::SEMANTIC_SCHEMA_VERSION,
        "schema_blake3": generated_types::SCHEMA_BLAKE3,
        "generator_blake3": generated_types::GENERATOR_BLAKE3,
        "generation": unsafe { (*runtime).generation },
        "fast_view_abi": cfg!(feature = "fast-view-abi"),
        "function_count": generated_table::FUNCTION_COUNT,
        "functions": {
            "runtimeNoop": generated_exports::iyon_runtime_noop_v1 as *const () as usize as u64,
            "viewRenderRef": generated_exports::iyon_view_render_ref_v1 as *const () as usize as u64,
            "viewSpacerCreate": generated_exports::iyon_view_spacer_create_v1 as *const () as usize as u64,
            "viewTextLayoutPatchRoot": generated_exports::iyon_view_text_layout_patch_root_v1 as *const () as usize as u64,
            "viewCommonPatchRoot": generated_exports::iyon_view_common_patch_root_v1 as *const () as usize as u64,
            "viewAxisCreateBuffer": generated_exports::iyon_view_axis_create_buffer_v1 as *const () as usize as u64,
            "viewReleaseMany": generated_exports::iyon_view_release_many_v1 as *const () as usize as u64,
            "viewRefForNodeId": generated_exports::iyon_view_ref_for_node_id_v1 as *const () as usize as u64,
        },
    }))
}

fn runtime_mut(pointer: *mut NativeViewRuntime) -> Result<&'static mut NativeViewRuntime, u32> {
    let runtime = unsafe { pointer.as_mut() }.ok_or(FAST_INVALID)?;
    if !runtime.valid_on_owner_thread() {
        return Err(FAST_INVALID);
    }
    Ok(runtime)
}

fn node_id(low: u32, high: u32) -> Result<u64, u32> {
    if high > 0x001f_ffff || (high == 0 && low == 0) {
        return Err(FAST_INVALID);
    }
    Ok((u64::from(high) << 32) | u64::from(low))
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn runtime_noop_impl(runtime: *mut NativeViewRuntime) -> u32 {
    runtime_mut(runtime).map(|_| 1).unwrap_or(FAST_INVALID)
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn view_render_ref_impl(
    runtime: *mut NativeViewRuntime,
    base: u32,
) -> u32 {
    let Ok(runtime) = runtime_mut(runtime) else {
        return FAST_INVALID;
    };
    match runtime.resolve_ref(base) {
        Ok(_) => base,
        Err(error) => error,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn view_ref_for_node_id_impl(
    runtime: *mut NativeViewRuntime,
    node_id_low: u32,
    node_id_high: u32,
) -> u32 {
    let Ok(runtime) = runtime_mut(runtime) else {
        return FAST_INVALID;
    };
    let Ok(node_id) = node_id(node_id_low, node_id_high) else {
        return FAST_INVALID;
    };
    match runtime.ref_for_node_id(node_id) {
        Ok(reference) => reference,
        Err(error) => error,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn view_spacer_create_impl(
    runtime: *mut NativeViewRuntime,
    node_id_low: u32,
    node_id_high: u32,
    rows: u32,
) -> u32 {
    let Ok(runtime) = runtime_mut(runtime) else {
        return FAST_INVALID;
    };
    let Ok(node_id) = node_id(node_id_low, node_id_high) else {
        return FAST_INVALID;
    };
    let Ok(rows) = u16::try_from(rows) else {
        return FAST_INVALID;
    };
    match runtime.publish(node_id, View::spacer(rows)) {
        Ok(reference) => reference,
        Err(error) => error,
    }
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
    let Ok(runtime) = runtime_mut(runtime) else {
        return FAST_INVALID;
    };
    let Ok(node_id) = node_id(node_id_low, node_id_high) else {
        return FAST_INVALID;
    };
    let Ok(wrap) = decode_wrap(wrap) else {
        return FAST_INVALID;
    };
    let Ok(align) = decode_align(align) else {
        return FAST_INVALID;
    };
    let Ok(base_view) = runtime.resolve_ref(base) else {
        return FAST_CACHE_MISS;
    };
    let Ok(patched) = base_view.try_with_text_layout_patch(Some(wrap), Some(align)) else {
        return FAST_INVALID;
    };
    match runtime.publish(node_id, patched) {
        Ok(reference) => reference,
        Err(error) => error,
    }
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
    let Ok(runtime) = runtime_mut(runtime) else {
        return FAST_INVALID;
    };
    if mask == 0 || mask & !PATCH_MASK != 0 {
        return FAST_INVALID;
    }
    let Ok(node_id) = node_id(node_id_low, node_id_high) else {
        return FAST_INVALID;
    };
    if runtime.resolve_ref(decoration_ref).is_err() {
        return FAST_CACHE_MISS;
    }
    let Ok(base_view) = runtime.resolve_ref(base) else {
        return FAST_CACHE_MISS;
    };
    let mut patched = base_view;
    if mask & PATCH_PADDING != 0 {
        patched = patched.padding(Insets::new(
            (padding_tr & 0xffff) as u16,
            (padding_tr >> 16) as u16,
            (padding_bl & 0xffff) as u16,
            (padding_bl >> 16) as u16,
        ));
    }
    if mask & PATCH_WIDTH != 0 {
        patched = match width_rule {
            1 => patched.fit_width(),
            2 => patched.fill_width(),
            _ => return FAST_INVALID,
        };
    }
    if mask & PATCH_HEIGHT != 0 {
        patched = match height_rule {
            1 => patched.fit_height(),
            2 => patched.fill_height(),
            _ => return FAST_INVALID,
        };
    }
    if mask & PATCH_MIN_WIDTH != 0 {
        let Ok(value) = u16::try_from(min_width) else {
            return FAST_INVALID;
        };
        patched = patched.min_width(value);
    }
    if mask & PATCH_MAX_WIDTH != 0 {
        let Ok(value) = u16::try_from(max_width) else {
            return FAST_INVALID;
        };
        patched = patched.max_width(value);
    }
    if mask & PATCH_MIN_HEIGHT != 0 {
        let Ok(value) = u16::try_from(min_height) else {
            return FAST_INVALID;
        };
        patched = patched.min_height(value);
    }
    if mask & PATCH_MAX_HEIGHT != 0 {
        let Ok(value) = u16::try_from(max_height) else {
            return FAST_INVALID;
        };
        patched = patched.max_height(value);
    }
    match runtime.publish(node_id, patched) {
        Ok(reference) => reference,
        Err(error) => error,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn view_axis_create_buffer_impl(
    runtime: *mut NativeViewRuntime,
    _node_id_low: u32,
    _node_id_high: u32,
    _axis_kind: u32,
    _gap: u32,
    _children: *const AxisChildInputV1,
    _children_capacity_bytes: usize,
    _used_child_count: u32,
) -> u32 {
    let Ok(_) = runtime_mut(runtime) else {
        return FAST_INVALID;
    };
    FAST_FALLBACK
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn view_release_many_impl(
    runtime: *mut NativeViewRuntime,
    refs: *const u32,
    _refs_capacity_bytes: usize,
    used_ref_count: u32,
) -> i32 {
    let Ok(runtime) = runtime_mut(runtime) else {
        return -1;
    };
    runtime.release_many(refs, used_ref_count).unwrap_or(-1)
}

fn decode_wrap(value: u32) -> Result<WrapMode, ()> {
    match value {
        1 => Ok(WrapMode::WordThenGrapheme),
        2 => Ok(WrapMode::Grapheme),
        3 => Ok(WrapMode::NoWrap),
        _ => Err(()),
    }
}

fn decode_align(value: u32) -> Result<HorizontalAlign, ()> {
    match value {
        1 => Ok(HorizontalAlign::Start),
        2 => Ok(HorizontalAlign::Center),
        3 => Ok(HorizontalAlign::End),
        _ => Err(()),
    }
}

#[cfg(test)]
mod tests {
    use super::{FAST_CACHE_MISS, NativeViewRuntime, generated_exports};
    use crate::tui::ViewBridgeCache;
    use iyon_tui::{IntoView, View};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    fn runtime() -> NativeViewRuntime {
        NativeViewRuntime::new(Arc::new(Mutex::new(ViewBridgeCache {
            nodes: HashMap::new(),
        })))
    }

    #[test]
    fn generated_spacer_publish_lookup_and_release_share_the_semantic_cache() {
        let mut runtime = runtime();
        let pointer = &mut runtime as *mut NativeViewRuntime;
        let reference = unsafe { generated_exports::iyon_view_spacer_create_v1(pointer, 7, 0, 2) };
        assert!(reference < 0x8000_0000);
        assert_eq!(
            unsafe { generated_exports::iyon_view_render_ref_v1(pointer, reference) },
            reference
        );
        assert_eq!(
            unsafe { generated_exports::iyon_view_ref_for_node_id_v1(pointer, 7, 0) },
            reference
        );
        assert_eq!(
            unsafe { generated_exports::iyon_view_release_many_v1(pointer, &reference, 4, 1) },
            1
        );
        assert_eq!(
            unsafe { generated_exports::iyon_view_render_ref_v1(pointer, reference) },
            FAST_CACHE_MISS
        );
    }

    #[test]
    fn generated_text_and_common_patches_publish_new_node_ids() {
        let mut runtime = runtime();
        let base = runtime
            .publish(1, View::text("hello").into_view())
            .expect("base ref");
        let pointer = &mut runtime as *mut NativeViewRuntime;
        let patched = unsafe {
            generated_exports::iyon_view_text_layout_patch_root_v1(pointer, base, 2, 0, 3, 2)
        };
        assert!(patched < 0x8000_0000);
        let common = unsafe {
            generated_exports::iyon_view_common_patch_root_v1(
                pointer, patched, 3, 0, 32, 0, 0, 0, 0, 4, 20, 0, 24, base,
            )
        };
        assert!(common < 0x8000_0000);
        assert_ne!(base, patched);
        assert_ne!(patched, common);
    }
}

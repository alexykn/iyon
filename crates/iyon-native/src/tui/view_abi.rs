use super::NativeTuiHost;
use crate::NativeError;
use iyon_tui::{HorizontalAlign, Insets, RetainedPathStep, View, WrapMode};
use napi::Env;
use napi_derive::napi;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::ThreadId;

#[cfg(any(feature = "perf-packed-benchmark", feature = "perf-packed-timing"))]
use super::fast_shared;
#[cfg(any(feature = "perf-packed-benchmark", feature = "perf-packed-timing"))]
use super::{packed_v3, packed_v4};

#[path = "../generated/view_abi_types.rs"]
mod generated_types;
pub use generated_types::AxisChildInputV1;

// The generated ABI keeps the host handle opaque. It is the stable N-API
// NativeTuiHost allocation, not the movable inner TuiHost value.
pub(super) type NativeHost = NativeTuiHost;

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
const FAST_INTERNAL: u32 = 0x8000_0006;
const HOST_STATUS_OK: i32 = 0;
const HOST_STATUS_CACHE_MISS: i32 = 1;
const HOST_STATUS_INVALID: i32 = -1;
const HOST_STATUS_INTERNAL: i32 = -3;

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

#[repr(C)]
pub(super) struct FastStatusCell {
    pub(super) code: AtomicU32,
    pub(super) detail: AtomicU32,
}

impl FastStatusCell {
    fn new() -> Self {
        Self {
            code: AtomicU32::new(0),
            detail: AtomicU32::new(0),
        }
    }

    fn record(&self, code: u32, detail: u32) -> u32 {
        self.detail.store(detail, Ordering::Release);
        self.code.store(code, Ordering::Release);
        code
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeViewKindTag {
    View = 1,
}

struct NativeViewSlot {
    node_id: u64,
    weak: iyon_tui::WeakView,
    leased: Option<View>,
    js_lease_count: u32,
    kind: NativeViewKindTag,
}

// PathRefs occupy a disjoint valid-handle range so a ViewRef can never be
// accepted as a path handle (and vice versa).
const PATH_ROOT_REF: u32 = 0x4000_0001;
const EDIT_TXN_REF_START: u32 = 0x7fff_0001;
const PATH_REF_LIMIT: u32 = EDIT_TXN_REF_START;
const EDIT_TXN_REF_LIMIT: u32 = 0x8000_0000;
const MAX_PATH_DEPTH: u32 = 128;
const MAX_EDIT_COUNT: u32 = 256;
const MAX_TXN_STAGED_OBJECTS: usize = 4_096;
const MAX_NEW_TEXT_BYTES: u32 = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct PathKey {
    parent: u32,
    kind: u32,
    expected_view_kind: u32,
    selector: u32,
}

#[derive(Clone, Copy, Debug)]
struct PathNode {
    parent: u32,
    step: RetainedPathStep,
    depth: u32,
}

#[derive(Clone)]
struct TextLayoutEdit {
    path_ref: u32,
    path_depth: u32,
    // IDs are ordered from changed leaf toward the changed root. Unused
    // entries are zero and are never interpreted by the transaction.
    node_ids: [u64; 5],
    wrap: WrapMode,
    align: HorizontalAlign,
}

struct EditTrieNode {
    step: Option<RetainedPathStep>,
    node_id: Option<u64>,
    edit: Option<TextLayoutEdit>,
    children: Vec<usize>,
}

struct EditTxn {
    base_root_ref: u32,
    base_view: View,
    expected_edit_count: u32,
    staged_text_bytes: u32,
    edits: Vec<TextLayoutEdit>,
}

struct StagedPublicationEntry {
    node_id: u64,
    view: View,
    reference: u32,
}

struct StagedPublication {
    entries: Vec<StagedPublicationEntry>,
    next_native_ref: u32,
}

#[repr(C)]
pub(super) struct NativeViewRuntime {
    pub magic: u32,
    pub abi_version: u32,
    pub semantic_version: u32,
    pub alive: AtomicU32,
    pub(super) status: FastStatusCell,
    owner_thread: ThreadId,
    // The semantic cache is deliberately owned by the environment runtime,
    // not by a transport or host. All direct, packed, FastShared, and
    // generated paths publish through this map.
    pub(super) nodes: HashMap<u64, iyon_tui::WeakView>,
    slots: HashMap<u32, NativeViewSlot>,
    node_refs: HashMap<u64, u32>,
    path_nodes: HashMap<u32, PathNode>,
    path_keys: HashMap<PathKey, u32>,
    edit_txns: HashMap<u32, EditTxn>,
    next_native_ref: u32,
    next_path_ref: u32,
    next_edit_txn_ref: u32,
    pub(super) generation: u32,
    #[cfg(any(feature = "perf-packed-benchmark", feature = "perf-packed-timing"))]
    pub(super) packed_v3: packed_v3::PackedState,
    #[cfg(any(feature = "perf-packed-benchmark", feature = "perf-packed-timing"))]
    pub(super) packed_v4: packed_v4::PackedState,
    #[cfg(any(feature = "perf-packed-benchmark", feature = "perf-packed-timing"))]
    pub(super) fast_slots: HashMap<usize, fast_shared::FastSlotTable>,
    #[cfg(any(feature = "perf-packed-benchmark", feature = "perf-packed-timing"))]
    pub(super) fast_sessions: HashMap<usize, usize>,
}

impl NativeViewRuntime {
    pub(super) fn new() -> Self {
        Self {
            magic: ABI_MAGIC,
            abi_version: ABI_VERSION,
            semantic_version: SEMANTIC_VERSION,
            alive: AtomicU32::new(1),
            status: FastStatusCell::new(),
            owner_thread: std::thread::current().id(),
            nodes: HashMap::new(),
            slots: HashMap::new(),
            node_refs: HashMap::new(),
            path_nodes: HashMap::from([(
                PATH_ROOT_REF,
                PathNode {
                    parent: 0,
                    step: RetainedPathStep::new(0, 0, 0),
                    depth: 0,
                },
            )]),
            path_keys: HashMap::new(),
            edit_txns: HashMap::new(),
            next_native_ref: 1,
            next_path_ref: PATH_ROOT_REF + 1,
            next_edit_txn_ref: EDIT_TXN_REF_LIMIT - 1,
            generation: 1,
            #[cfg(any(feature = "perf-packed-benchmark", feature = "perf-packed-timing"))]
            packed_v3: packed_v3::PackedState::new(),
            #[cfg(any(feature = "perf-packed-benchmark", feature = "perf-packed-timing"))]
            packed_v4: packed_v4::PackedState::new(),
            #[cfg(any(feature = "perf-packed-benchmark", feature = "perf-packed-timing"))]
            fast_slots: HashMap::new(),
            #[cfg(any(feature = "perf-packed-benchmark", feature = "perf-packed-timing"))]
            fast_sessions: HashMap::new(),
        }
    }

    #[cfg(any(feature = "perf-packed-benchmark", feature = "perf-packed-timing"))]
    pub(super) fn fast_slots_for(&mut self, host_addr: usize) -> &mut fast_shared::FastSlotTable {
        self.fast_slots
            .entry(host_addr)
            .or_insert_with(fast_shared::FastSlotTable::new)
    }

    pub(super) fn valid_on_owner_thread(&self) -> bool {
        self.magic == ABI_MAGIC
            && self.abi_version == ABI_VERSION
            && self.semantic_version == SEMANTIC_VERSION
            && self.alive.load(Ordering::Acquire) != 0
            && self.owner_thread == std::thread::current().id()
    }

    fn allocate_path_ref(&mut self) -> Option<u32> {
        while self.next_path_ref < PATH_REF_LIMIT {
            let candidate = self.next_path_ref;
            self.next_path_ref = self.next_path_ref.wrapping_add(1);
            if candidate != 0 && !self.path_nodes.contains_key(&candidate) {
                return Some(candidate);
            }
        }
        None
    }

    fn path_root(&mut self) -> u32 {
        PATH_ROOT_REF
    }

    fn path_child(
        &mut self,
        parent: u32,
        kind: u32,
        expected_view_kind: u32,
        selector: u32,
    ) -> Result<u32, u32> {
        if !is_valid_path_ref(parent) {
            return Err(FAST_INVALID);
        }
        let Some(parent_node) = self.path_nodes.get(&parent).copied() else {
            return Err(FAST_CACHE_MISS);
        };
        if !(1..=9).contains(&kind)
            || !(1..=8).contains(&expected_view_kind)
            || !path_step_matches_kind(kind, expected_view_kind)
            || parent_node.depth >= MAX_PATH_DEPTH
            || selector > 1_000_000
        {
            return Err(FAST_INVALID);
        }
        let key = PathKey {
            parent,
            kind,
            expected_view_kind,
            selector,
        };
        if let Some(reference) = self.path_keys.get(&key).copied() {
            return Ok(reference);
        }
        let reference = self.allocate_path_ref().ok_or(FAST_FALLBACK)?;
        self.path_keys.insert(key, reference);
        self.path_nodes.insert(
            reference,
            PathNode {
                parent,
                step: RetainedPathStep::new(kind, expected_view_kind, selector),
                depth: parent_node.depth + 1,
            },
        );
        Ok(reference)
    }

    fn path_steps(&self, reference: u32) -> Result<Vec<RetainedPathStep>, u32> {
        if !is_valid_path_ref(reference) {
            return Err(FAST_INVALID);
        }
        let Some(node) = self.path_nodes.get(&reference).copied() else {
            return Err(FAST_CACHE_MISS);
        };
        let mut steps = Vec::with_capacity(node.depth as usize);
        let mut current = reference;
        while current != PATH_ROOT_REF {
            let Some(node) = self.path_nodes.get(&current).copied() else {
                return Err(FAST_CACHE_MISS);
            };
            steps.push(node.step);
            current = node.parent;
        }
        steps.reverse();
        Ok(steps)
    }

    fn allocate_edit_txn_ref(&mut self) -> Option<u32> {
        while self.next_edit_txn_ref >= EDIT_TXN_REF_START {
            let candidate = self.next_edit_txn_ref;
            self.next_edit_txn_ref = self.next_edit_txn_ref.saturating_sub(1);
            if candidate != 0 && !self.edit_txns.contains_key(&candidate) {
                return Some(candidate);
            }
        }
        None
    }

    fn begin_edit_txn(&mut self, base_root_ref: u32, expected_edit_count: u32) -> Result<u32, u32> {
        if !is_valid_view_ref(base_root_ref)
            || expected_edit_count == 0
            || expected_edit_count > MAX_EDIT_COUNT
        {
            return Err(FAST_INVALID);
        }
        let Ok((base_view, _)) = self.resolve_ref(base_root_ref) else {
            return Err(FAST_CACHE_MISS);
        };
        let reference = self.allocate_edit_txn_ref().ok_or(FAST_FALLBACK)?;
        self.edit_txns.insert(
            reference,
            EditTxn {
                base_root_ref,
                base_view,
                expected_edit_count,
                staged_text_bytes: 0,
                edits: Vec::with_capacity(expected_edit_count as usize),
            },
        );
        Ok(reference)
    }

    fn add_text_layout_edit(
        &mut self,
        txn_ref: u32,
        path_ref: u32,
        path_depth: u32,
        node_ids: [u64; 5],
        wrap: WrapMode,
        align: HorizontalAlign,
    ) -> i32 {
        if !is_valid_edit_txn_ref(txn_ref) {
            return -1;
        }
        if path_depth > 4 || path_depth > MAX_PATH_DEPTH {
            return 2;
        }
        let Ok(steps) = self.path_steps(path_ref) else {
            return 1;
        };
        if steps.len() != path_depth as usize {
            return -1;
        }
        let Some(txn) = self.edit_txns.get_mut(&txn_ref) else {
            return 1;
        };
        if txn.edits.len() as u32 >= txn.expected_edit_count
            || txn.edits.len() as u32 >= MAX_EDIT_COUNT
            || txn.staged_text_bytes > MAX_NEW_TEXT_BYTES
            || (txn.edits.len() + path_depth as usize + 1) > MAX_TXN_STAGED_OBJECTS
        {
            return 2;
        }
        if txn
            .edits
            .iter()
            .any(|edit| edit.path_ref == path_ref && edit.path_depth == path_depth)
        {
            return -1;
        }
        txn.edits.push(TextLayoutEdit {
            path_ref,
            path_depth,
            node_ids,
            wrap,
            align,
        });
        0
    }

    fn build_edit_trie(&self, txn: &EditTxn) -> Result<Vec<EditTrieNode>, u32> {
        if txn.edits.is_empty() {
            return Err(FAST_INVALID);
        }
        let mut trie = vec![EditTrieNode {
            step: None,
            node_id: None,
            edit: None,
            children: Vec::new(),
        }];
        for edit in &txn.edits {
            let steps = self.path_steps(edit.path_ref)?;
            if steps.len() != edit.path_depth as usize || edit.path_depth > 4 {
                return Err(FAST_INVALID);
            }
            let root_id = edit.node_ids[edit.path_depth as usize];
            if root_id == 0 {
                return Err(FAST_INVALID);
            }
            set_trie_node_id(&mut trie[0], root_id)?;
            let mut current = 0;
            for (index, step) in steps.iter().copied().enumerate() {
                let child = trie[current]
                    .children
                    .iter()
                    .copied()
                    .find(|candidate| trie[*candidate].step == Some(step));
                let child = if let Some(child) = child {
                    child
                } else {
                    if trie.len() >= MAX_TXN_STAGED_OBJECTS {
                        return Err(FAST_FALLBACK);
                    }
                    let child = trie.len();
                    trie.push(EditTrieNode {
                        step: Some(step),
                        node_id: None,
                        edit: None,
                        children: Vec::new(),
                    });
                    trie[current].children.push(child);
                    child
                };
                let node_id = edit.node_ids[edit.path_depth as usize - index - 1];
                if node_id == 0 {
                    return Err(FAST_INVALID);
                }
                set_trie_node_id(&mut trie[child], node_id)?;
                current = child;
            }
            if trie[current].edit.is_some() || !trie[current].children.is_empty() {
                return Err(FAST_INVALID);
            }
            trie[current].edit = Some(edit.clone());
        }
        Ok(trie)
    }

    fn stage_edit_trie(
        &self,
        view: View,
        trie: &[EditTrieNode],
        index: usize,
        staged: &mut Vec<(u64, View)>,
    ) -> Result<View, u32> {
        let node = &trie[index];
        if let Some(edit) = node.edit.as_ref() {
            if !node.children.is_empty() || node.node_id.is_none() {
                return Err(FAST_INVALID);
            }
            let patched = view
                .try_with_text_layout_patch(Some(edit.wrap), Some(edit.align))
                .map_err(|_| FAST_INVALID)?;
            staged.push((node.node_id.unwrap(), patched.clone()));
            return Ok(patched);
        }
        if node.children.is_empty() || node.node_id.is_none() {
            return Err(FAST_INVALID);
        }
        let mut patched = view.clone();
        for &child_index in &node.children {
            let step = trie[child_index].step.ok_or(FAST_INVALID)?;
            let child = view.try_retained_child(step).map_err(|_| FAST_INVALID)?;
            let rebuilt = self.stage_edit_trie(child, trie, child_index, staged)?;
            patched = patched
                .try_replace_retained_child(step, rebuilt)
                .map_err(|_| FAST_INVALID)?;
        }
        staged.push((node.node_id.unwrap(), patched.clone()));
        Ok(patched)
    }

    /// Validates all logical publication failures and reserves the NativeRefs
    /// without exposing them. The returned plan is committed only after the
    /// host accepts the new root, so a host error cannot leave published refs
    /// for an uninstalled View.
    fn prepare_staged_publication(
        &mut self,
        staged: Vec<(u64, View)>,
    ) -> Result<StagedPublication, u32> {
        let mut unique = HashSet::with_capacity(staged.len());
        let mut planned_refs = HashSet::with_capacity(staged.len());
        let mut next_native_ref = self.next_native_ref;
        let mut entries = Vec::with_capacity(staged.len());

        for (node_id, view) in staged {
            if node_id == 0 || !unique.insert(node_id) {
                return Err(FAST_INVALID);
            }
            if let Some(existing) = self
                .nodes
                .get(&node_id)
                .and_then(iyon_tui::WeakView::upgrade)
                && existing != view
            {
                return Err(FAST_INVALID);
            }

            let reference = if let Some(reference) = self.node_refs.get(&node_id).copied() {
                match self.resolve_ref(reference) {
                    Ok((existing, _)) if existing != view => return Err(FAST_INVALID),
                    Ok(_) => reference,
                    Err(FAST_CACHE_MISS) => {
                        self.node_refs.remove(&node_id);
                        reserve_staged_ref(&self.slots, &mut planned_refs, &mut next_native_ref)?
                    }
                    Err(error) => return Err(error),
                }
            } else {
                reserve_staged_ref(&self.slots, &mut planned_refs, &mut next_native_ref)?
            };
            entries.push(StagedPublicationEntry {
                node_id,
                view,
                reference,
            });
        }

        if entries.is_empty() {
            return Err(FAST_INVALID);
        }
        Ok(StagedPublication {
            entries,
            next_native_ref,
        })
    }

    /// Commits a previously prepared plan. All semantic error conditions were
    /// checked before host installation; this phase only installs the plan's
    /// already-reserved entries and cannot return a recoverable ABI status.
    fn commit_staged_publication(&mut self, publication: StagedPublication) -> u32 {
        let root_ref = publication
            .entries
            .last()
            .map(|entry| entry.reference)
            .unwrap_or(0);
        let last_index = publication.entries.len().saturating_sub(1);
        self.next_native_ref = publication.next_native_ref;
        for (index, entry) in publication.entries.into_iter().enumerate() {
            let is_root = index == last_index;
            if self.node_refs.get(&entry.node_id) == Some(&entry.reference) {
                if is_root {
                    let _ = self.ensure_lease(entry.reference, entry.view);
                }
                continue;
            }
            let weak = entry.view.downgrade();
            self.nodes.insert(entry.node_id, weak.clone());
            self.node_refs.insert(entry.node_id, entry.reference);
            self.slots.insert(
                entry.reference,
                NativeViewSlot {
                    node_id: entry.node_id,
                    weak,
                    leased: is_root.then_some(entry.view),
                    js_lease_count: u32::from(is_root),
                    kind: NativeViewKindTag::View,
                },
            );
        }
        root_ref
    }

    fn allocate_ref(&mut self) -> Option<u32> {
        while self.next_native_ref < PATH_ROOT_REF {
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
        if slot.kind != NativeViewKindTag::View {
            return Err(FAST_INVALID);
        }
        if slot.js_lease_count == 0 {
            slot.leased = Some(view);
            slot.js_lease_count = 1;
        } else if slot.leased.is_none() {
            slot.leased = Some(view);
        }
        Ok(())
    }

    fn acquire_lease(&mut self, reference: u32, view: View) -> Result<(), u32> {
        let Some(slot) = self.slots.get_mut(&reference) else {
            return Err(FAST_CACHE_MISS);
        };
        if slot.kind != NativeViewKindTag::View {
            return Err(FAST_INVALID);
        }
        let Some(count) = slot.js_lease_count.checked_add(1) else {
            return Err(FAST_FALLBACK);
        };
        if slot.leased.is_none() {
            slot.leased = Some(view);
        }
        slot.js_lease_count = count;
        Ok(())
    }

    fn resolve_ref(&mut self, reference: u32) -> Result<(View, bool), u32> {
        let Some(slot) = self.slots.get_mut(&reference) else {
            return Err(FAST_CACHE_MISS);
        };
        if slot.kind != NativeViewKindTag::View {
            return Err(FAST_INVALID);
        }
        if let Some(view) = slot.leased.clone() {
            return Ok((view, true));
        }
        let Some(view) = slot.weak.upgrade() else {
            let node_id = slot.node_id;
            self.node_refs.remove(&node_id);
            self.slots.remove(&reference);
            return Err(FAST_CACHE_MISS);
        };
        Ok((view, false))
    }

    fn publish(&mut self, node_id: u64, view: View) -> Result<u32, u32> {
        if node_id == 0 {
            return Err(FAST_INVALID);
        }
        if let Some(reference) = self.node_refs.get(&node_id).copied() {
            match self.resolve_ref(reference) {
                Ok((existing, has_lease)) if existing == view => {
                    if !has_lease {
                        self.ensure_lease(reference, existing)?;
                    }
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
        if let Some(existing) = self
            .nodes
            .get(&node_id)
            .and_then(iyon_tui::WeakView::upgrade)
            && existing != view
        {
            return Err(FAST_INVALID);
        }
        self.nodes.insert(node_id, weak.clone());
        self.node_refs.insert(node_id, reference);
        self.slots.insert(
            reference,
            NativeViewSlot {
                node_id,
                weak,
                leased: Some(view),
                js_lease_count: 1,
                kind: NativeViewKindTag::View,
            },
        );
        Ok(reference)
    }

    // Bulk V2/V3/V4 and FastShared definitions do not represent a live JS
    // backing, so they receive a weak-only lease. The generated path can
    // reacquire the same NativeRef later through the semantic NodeId cache.
    pub(super) fn publish_bulk(&mut self, node_id: u64, view: View) -> Result<u32, u32> {
        if node_id == 0 {
            return Err(FAST_INVALID);
        }
        if let Some(reference) = self.node_refs.get(&node_id).copied() {
            match self.resolve_ref(reference) {
                Ok((existing, _)) if existing == view => return Ok(reference),
                Ok(_) => return Err(FAST_INVALID),
                Err(FAST_CACHE_MISS) => {
                    self.node_refs.remove(&node_id);
                }
                Err(error) => return Err(error),
            }
        }
        if let Some(existing) = self
            .nodes
            .get(&node_id)
            .and_then(iyon_tui::WeakView::upgrade)
            && existing != view
        {
            return Err(FAST_INVALID);
        }
        let reference = self.allocate_ref().ok_or(FAST_FALLBACK)?;
        let weak = view.downgrade();
        self.nodes.insert(node_id, weak.clone());
        self.node_refs.insert(node_id, reference);
        self.slots.insert(
            reference,
            NativeViewSlot {
                node_id,
                weak,
                leased: None,
                js_lease_count: 0,
                kind: NativeViewKindTag::View,
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
                Ok((view, has_lease)) => {
                    if has_lease {
                        self.acquire_lease(reference, view)?;
                    } else {
                        self.ensure_lease(reference, view)?;
                    }
                    return Ok(reference);
                }
                Err(_) => {
                    self.node_refs.remove(&node_id);
                }
            }
        }
        let Some(weak) = self.nodes.get(&node_id).cloned() else {
            return Err(FAST_CACHE_MISS);
        };
        let Some(view) = weak.upgrade() else {
            self.nodes.remove(&node_id);
            return Err(FAST_CACHE_MISS);
        };
        self.publish(node_id, view)
    }

    fn abort_all_edit_txns(&mut self) {
        self.edit_txns.clear();
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

pub(super) type ViewRuntimeHandle = Arc<NativeViewRuntime>;

static RUNTIME_HANDLES: OnceLock<Mutex<HashMap<usize, ViewRuntimeHandle>>> = OnceLock::new();

fn runtime_handles() -> &'static Mutex<HashMap<usize, ViewRuntimeHandle>> {
    RUNTIME_HANDLES.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) fn runtime_handle_for_env(env: &Env) -> napi::Result<ViewRuntimeHandle> {
    let env_key = env.raw() as usize;
    let mut handles = runtime_handles()
        .lock()
        .map_err(|_| NativeError::internal("native View ABI runtime registry is poisoned"))?;
    if let Some(runtime) = handles.get(&env_key) {
        return Ok(Arc::clone(runtime));
    }
    let runtime = Arc::new(NativeViewRuntime::new());
    let cleanup_key = env_key;
    let cleanup_runtime = Arc::clone(&runtime);
    env.add_env_cleanup_hook(cleanup_key, move |_| {
        cleanup_runtime.alive.store(0, Ordering::Release);
        if let Some(registry) = RUNTIME_HANDLES.get()
            && let Ok(mut handles) = registry.lock()
        {
            handles.remove(&cleanup_key);
        }
    })?;
    handles.insert(env_key, Arc::clone(&runtime));
    Ok(runtime)
}

pub(super) fn runtime_ptr_for_env(env: &Env) -> napi::Result<*mut NativeViewRuntime> {
    let runtime = runtime_handle_for_env(env)?;
    Ok(Arc::as_ptr(&runtime) as *mut NativeViewRuntime)
}

pub(super) fn runtime_environment_count() -> i64 {
    RUNTIME_HANDLES
        .get()
        .and_then(|handles| handles.lock().ok())
        .map(|handles| handles.len() as i64)
        .unwrap_or(0)
}

pub(super) fn runtime_is_registered(pointer: usize) -> bool {
    RUNTIME_HANDLES
        .get()
        .and_then(|handles| handles.lock().ok())
        .is_some_and(|handles| {
            handles
                .values()
                .any(|runtime| Arc::as_ptr(runtime) as usize == pointer)
        })
}

pub(super) fn runtime_from_handle(
    handle: &ViewRuntimeHandle,
) -> napi::Result<&'static mut NativeViewRuntime> {
    let runtime = unsafe { (Arc::as_ptr(handle) as *mut NativeViewRuntime).as_mut() }
        .ok_or_else(|| NativeError::internal("native View runtime pointer is null"))?;
    if !runtime.valid_on_owner_thread() {
        return Err(NativeError::coded(
            napi::Status::Closing,
            "ION_VIEW_RUNTIME_INVALID",
            "native View runtime is disposed or called from the wrong thread",
        ));
    }
    Ok(runtime)
}

pub(super) fn runtime_for_env(env: &Env) -> napi::Result<*mut NativeViewRuntime> {
    runtime_ptr_for_env(env)
}

/// Host disposal must discard any transaction that could otherwise retain a
/// strong staged root until environment teardown. Transactions are runtime-
/// scoped (the ABI begin call intentionally has no host argument), so clearing
/// the environment's uncommitted set is the conservative lifecycle boundary.
pub(super) fn abort_all_edit_txns(pointer: *mut NativeViewRuntime) {
    let Ok(runtime) = runtime_mut(pointer) else {
        return;
    };
    runtime.abort_all_edit_txns();
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
            "hostRenderRef": generated_exports::iyon_host_render_ref_v1 as *const () as usize as u64,
            "viewSpacerCreate": generated_exports::iyon_view_spacer_create_v1 as *const () as usize as u64,
            "viewTextLayoutPatchRoot": generated_exports::iyon_view_text_layout_patch_root_v1 as *const () as usize as u64,
            "viewCommonPatchRoot": generated_exports::iyon_view_common_patch_root_v1 as *const () as usize as u64,
            "viewAxisCreateBuffer": generated_exports::iyon_view_axis_create_buffer_v1 as *const () as usize as u64,
            "viewAxisSetChild": generated_exports::iyon_view_axis_set_child_v1 as *const () as usize as u64,
            "viewAxisSpliceBuffer": generated_exports::iyon_view_axis_splice_buffer_v1 as *const () as usize as u64,
            "viewGridSetCell": generated_exports::iyon_view_grid_set_cell_v1 as *const () as usize as u64,
            "viewAxisSetChildPath": generated_exports::iyon_view_axis_set_child_path_v1 as *const () as usize as u64,
            "viewGridSetCellPath": generated_exports::iyon_view_grid_set_cell_path_v1 as *const () as usize as u64,
            "viewReleaseMany": generated_exports::iyon_view_release_many_v1 as *const () as usize as u64,
            "viewRefForNodeId": generated_exports::iyon_view_ref_for_node_id_v1 as *const () as usize as u64,
            "pathRoot": generated_exports::iyon_path_root_v1 as *const () as usize as u64,
            "pathChild": generated_exports::iyon_path_child_v1 as *const () as usize as u64,
            "viewTextLayoutPatchPath": generated_exports::iyon_view_text_layout_patch_path_v1 as *const () as usize as u64,
            "viewTextLayoutPatchPathD1": generated_exports::iyon_view_text_layout_patch_path_d1_v1 as *const () as usize as u64,
            "viewTextLayoutPatchPathD2": generated_exports::iyon_view_text_layout_patch_path_d2_v1 as *const () as usize as u64,
            "viewTextLayoutPatchPathD3": generated_exports::iyon_view_text_layout_patch_path_d3_v1 as *const () as usize as u64,
            "viewTextLayoutPatchPathD4": generated_exports::iyon_view_text_layout_patch_path_d4_v1 as *const () as usize as u64,
            "editTxnBegin": generated_exports::iyon_edit_txn_begin_v1 as *const () as usize as u64,
            "editTxnAddTextLayout": generated_exports::iyon_edit_txn_add_text_layout_v1 as *const () as usize as u64,
            "editTxnCommitRender": generated_exports::iyon_edit_txn_commit_render_v1 as *const () as usize as u64,
            "editTxnAbort": generated_exports::iyon_edit_txn_abort_v1 as *const () as usize as u64,
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

fn record_result(runtime: &NativeViewRuntime, result: u32) -> u32 {
    runtime.status.record(result, 0)
}

fn record_host_status(runtime: &NativeViewRuntime, status: i32) -> i32 {
    runtime.status.record(status as u32, 0);
    status
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn runtime_noop_impl(runtime: *mut NativeViewRuntime) -> u32 {
    let Ok(runtime) = runtime_mut(runtime) else {
        return FAST_INVALID;
    };
    record_result(runtime, 1)
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn view_render_ref_impl(
    runtime: *mut NativeViewRuntime,
    base: u32,
) -> u32 {
    let Ok(runtime) = runtime_mut(runtime) else {
        return FAST_INVALID;
    };
    let result = match runtime.resolve_ref(base) {
        Ok(_) => base,
        Err(error) => error,
    };
    record_result(runtime, result)
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn host_render_ref_impl(
    runtime: *mut NativeViewRuntime,
    host: *mut NativeHost,
    base: u32,
) -> i32 {
    let Ok(runtime) = runtime_mut(runtime) else {
        return HOST_STATUS_INVALID;
    };
    let status = {
        let Some(host) = (unsafe { host.as_ref() }) else {
            return record_host_status(runtime, HOST_STATUS_INVALID);
        };
        if !host.alive.load(Ordering::Acquire) {
            return record_host_status(runtime, HOST_STATUS_INVALID);
        }
        let Ok((view, _)) = runtime.resolve_ref(base) else {
            return record_host_status(runtime, HOST_STATUS_CACHE_MISS);
        };
        match host.host.render(view) {
            Ok(()) => HOST_STATUS_OK,
            Err(_) => HOST_STATUS_INTERNAL,
        }
    };
    record_host_status(runtime, status)
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
    let result = match runtime.ref_for_node_id(node_id) {
        Ok(reference) => reference,
        Err(error) => error,
    };
    record_result(runtime, result)
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn path_root_impl(runtime: *mut NativeViewRuntime) -> u32 {
    let Ok(runtime) = runtime_mut(runtime) else {
        return FAST_INVALID;
    };
    let reference = runtime.path_root();
    record_result(runtime, reference)
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn path_child_impl(
    runtime: *mut NativeViewRuntime,
    parent_path_ref: u32,
    step_kind: u32,
    expected_view_kind: u32,
    selector: u32,
) -> u32 {
    let Ok(runtime) = runtime_mut(runtime) else {
        return FAST_INVALID;
    };
    let result = runtime
        .path_child(parent_path_ref, step_kind, expected_view_kind, selector)
        .unwrap_or_else(|error| error);
    record_result(runtime, result)
}

fn publish_text_path(
    runtime: &mut NativeViewRuntime,
    base_root_ref: u32,
    path_ref: u32,
    path_depth: u32,
    node_ids: &[(u32, u32)],
    wrap: u32,
    align: u32,
) -> u32 {
    if path_depth > 4 || node_ids.len() != path_depth as usize + 1 {
        return FAST_INVALID;
    }
    if !is_valid_view_ref(base_root_ref) {
        return FAST_INVALID;
    }
    let steps = match runtime.path_steps(path_ref) {
        Ok(steps) => steps,
        Err(error) => return error,
    };
    if steps.len() != path_depth as usize {
        return FAST_INVALID;
    }
    let Ok(wrap) = decode_wrap(wrap) else {
        return FAST_INVALID;
    };
    let Ok(align) = decode_align(align) else {
        return FAST_INVALID;
    };
    let mut decoded_ids = Vec::with_capacity(node_ids.len());
    for &(low, high) in node_ids {
        let Ok(node_id) = node_id(low, high) else {
            return FAST_INVALID;
        };
        decoded_ids.push(node_id);
    }
    let Ok((base_view, _)) = runtime.resolve_ref(base_root_ref) else {
        return FAST_CACHE_MISS;
    };
    let Ok((root, views)) =
        base_view.try_with_text_layout_patch_path_with_nodes(&steps, wrap, align)
    else {
        return FAST_INVALID;
    };
    if views.len() != decoded_ids.len() || views.last() != Some(&root) {
        return FAST_INVALID;
    }
    if let Err(error) = validate_path_publication(runtime, &decoded_ids, &views) {
        return error;
    }
    let mut root_ref = 0;
    let last_index = views.len().saturating_sub(1);
    for (index, (node_id, view)) in decoded_ids.into_iter().zip(views).enumerate() {
        let result = if index == last_index {
            runtime.publish(node_id, view)
        } else {
            runtime.publish_bulk(node_id, view)
        };
        match result {
            Ok(reference) => root_ref = reference,
            Err(error) => return error,
        }
    }
    root_ref
}

fn validate_path_publication(
    runtime: &mut NativeViewRuntime,
    node_ids: &[u64],
    views: &[View],
) -> Result<(), u32> {
    let mut unique = std::collections::HashSet::with_capacity(node_ids.len());
    for (node_id, view) in node_ids.iter().copied().zip(views) {
        if !unique.insert(node_id) {
            return Err(FAST_INVALID);
        }
        if let Some(existing) = runtime
            .nodes
            .get(&node_id)
            .and_then(iyon_tui::WeakView::upgrade)
            && existing != *view
        {
            return Err(FAST_INVALID);
        }
        if let Some(reference) = runtime.node_refs.get(&node_id).copied()
            && let Ok((existing, _)) = runtime.resolve_ref(reference)
            && existing != *view
        {
            return Err(FAST_INVALID);
        }
    }
    if runtime.next_native_ref >= PATH_ROOT_REF.saturating_sub(node_ids.len() as u32) {
        return Err(FAST_FALLBACK);
    }
    Ok(())
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn view_text_layout_patch_path_impl(
    runtime: *mut NativeViewRuntime,
    base_root_ref: u32,
    path_ref: u32,
    path_depth: u32,
    target_node_id_low: u32,
    target_node_id_high: u32,
    ancestor0_node_id_low: u32,
    ancestor0_node_id_high: u32,
    ancestor1_node_id_low: u32,
    ancestor1_node_id_high: u32,
    ancestor2_node_id_low: u32,
    ancestor2_node_id_high: u32,
    ancestor3_node_id_low: u32,
    ancestor3_node_id_high: u32,
    wrap: u32,
    align: u32,
) -> u32 {
    let Ok(runtime) = runtime_mut(runtime) else {
        return FAST_INVALID;
    };
    if path_depth > 4 {
        return record_result(runtime, FAST_INVALID);
    }
    let node_ids = [
        (target_node_id_low, target_node_id_high),
        (ancestor0_node_id_low, ancestor0_node_id_high),
        (ancestor1_node_id_low, ancestor1_node_id_high),
        (ancestor2_node_id_low, ancestor2_node_id_high),
        (ancestor3_node_id_low, ancestor3_node_id_high),
    ];
    let result = publish_text_path(
        runtime,
        base_root_ref,
        path_ref,
        path_depth,
        &node_ids[..path_depth as usize + 1],
        wrap,
        align,
    );
    record_result(runtime, result)
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn view_text_layout_patch_path_d1_impl(
    runtime: *mut NativeViewRuntime,
    base_root_ref: u32,
    path_ref: u32,
    target_node_id_low: u32,
    target_node_id_high: u32,
    ancestor0_node_id_low: u32,
    ancestor0_node_id_high: u32,
    wrap: u32,
    align: u32,
) -> u32 {
    let Ok(runtime) = runtime_mut(runtime) else {
        return FAST_INVALID;
    };
    let result = publish_text_path(
        runtime,
        base_root_ref,
        path_ref,
        1,
        &[
            (target_node_id_low, target_node_id_high),
            (ancestor0_node_id_low, ancestor0_node_id_high),
        ],
        wrap,
        align,
    );
    record_result(runtime, result)
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn view_text_layout_patch_path_d2_impl(
    runtime: *mut NativeViewRuntime,
    base_root_ref: u32,
    path_ref: u32,
    target_node_id_low: u32,
    target_node_id_high: u32,
    ancestor0_node_id_low: u32,
    ancestor0_node_id_high: u32,
    ancestor1_node_id_low: u32,
    ancestor1_node_id_high: u32,
    wrap: u32,
    align: u32,
) -> u32 {
    let Ok(runtime) = runtime_mut(runtime) else {
        return FAST_INVALID;
    };
    let result = publish_text_path(
        runtime,
        base_root_ref,
        path_ref,
        2,
        &[
            (target_node_id_low, target_node_id_high),
            (ancestor0_node_id_low, ancestor0_node_id_high),
            (ancestor1_node_id_low, ancestor1_node_id_high),
        ],
        wrap,
        align,
    );
    record_result(runtime, result)
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn view_text_layout_patch_path_d3_impl(
    runtime: *mut NativeViewRuntime,
    base_root_ref: u32,
    path_ref: u32,
    target_node_id_low: u32,
    target_node_id_high: u32,
    ancestor0_node_id_low: u32,
    ancestor0_node_id_high: u32,
    ancestor1_node_id_low: u32,
    ancestor1_node_id_high: u32,
    ancestor2_node_id_low: u32,
    ancestor2_node_id_high: u32,
    wrap: u32,
    align: u32,
) -> u32 {
    let Ok(runtime) = runtime_mut(runtime) else {
        return FAST_INVALID;
    };
    let result = publish_text_path(
        runtime,
        base_root_ref,
        path_ref,
        3,
        &[
            (target_node_id_low, target_node_id_high),
            (ancestor0_node_id_low, ancestor0_node_id_high),
            (ancestor1_node_id_low, ancestor1_node_id_high),
            (ancestor2_node_id_low, ancestor2_node_id_high),
        ],
        wrap,
        align,
    );
    record_result(runtime, result)
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn view_text_layout_patch_path_d4_impl(
    runtime: *mut NativeViewRuntime,
    base_root_ref: u32,
    path_ref: u32,
    target_node_id_low: u32,
    target_node_id_high: u32,
    ancestor0_node_id_low: u32,
    ancestor0_node_id_high: u32,
    ancestor1_node_id_low: u32,
    ancestor1_node_id_high: u32,
    ancestor2_node_id_low: u32,
    ancestor2_node_id_high: u32,
    ancestor3_node_id_low: u32,
    ancestor3_node_id_high: u32,
    wrap: u32,
    align: u32,
) -> u32 {
    let Ok(runtime) = runtime_mut(runtime) else {
        return FAST_INVALID;
    };
    let result = publish_text_path(
        runtime,
        base_root_ref,
        path_ref,
        4,
        &[
            (target_node_id_low, target_node_id_high),
            (ancestor0_node_id_low, ancestor0_node_id_high),
            (ancestor1_node_id_low, ancestor1_node_id_high),
            (ancestor2_node_id_low, ancestor2_node_id_high),
            (ancestor3_node_id_low, ancestor3_node_id_high),
        ],
        wrap,
        align,
    );
    record_result(runtime, result)
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn edit_txn_begin_impl(
    runtime: *mut NativeViewRuntime,
    base_root_ref: u32,
    expected_edit_count: u32,
) -> u32 {
    let Ok(runtime) = runtime_mut(runtime) else {
        return FAST_INVALID;
    };
    let result = runtime
        .begin_edit_txn(base_root_ref, expected_edit_count)
        .unwrap_or_else(|error| error);
    record_result(runtime, result)
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn edit_txn_add_text_layout_impl(
    runtime: *mut NativeViewRuntime,
    txn_ref: u32,
    path_ref: u32,
    path_depth: u32,
    target_node_id_low: u32,
    target_node_id_high: u32,
    ancestor0_node_id_low: u32,
    ancestor0_node_id_high: u32,
    ancestor1_node_id_low: u32,
    ancestor1_node_id_high: u32,
    ancestor2_node_id_low: u32,
    ancestor2_node_id_high: u32,
    ancestor3_node_id_low: u32,
    ancestor3_node_id_high: u32,
    wrap: u32,
    align: u32,
) -> i32 {
    let Ok(runtime) = runtime_mut(runtime) else {
        return -1;
    };
    if path_depth > 4 {
        return record_host_status(runtime, 2);
    }
    let ids = [
        (target_node_id_low, target_node_id_high),
        (ancestor0_node_id_low, ancestor0_node_id_high),
        (ancestor1_node_id_low, ancestor1_node_id_high),
        (ancestor2_node_id_low, ancestor2_node_id_high),
        (ancestor3_node_id_low, ancestor3_node_id_high),
    ];
    let mut node_ids = [0_u64; 5];
    for index in 0..=path_depth as usize {
        let Ok(id) = node_id(ids[index].0, ids[index].1) else {
            return record_host_status(runtime, -1);
        };
        node_ids[index] = id;
    }
    let Ok(wrap) = decode_wrap(wrap) else {
        return record_host_status(runtime, -1);
    };
    let Ok(align) = decode_align(align) else {
        return record_host_status(runtime, -1);
    };
    let status = runtime.add_text_layout_edit(txn_ref, path_ref, path_depth, node_ids, wrap, align);
    record_host_status(runtime, status)
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn edit_txn_commit_render_impl(
    runtime: *mut NativeViewRuntime,
    host: *mut NativeHost,
    txn_ref: u32,
) -> u32 {
    let Ok(runtime) = runtime_mut(runtime) else {
        return FAST_INVALID;
    };
    let Some(host) = (unsafe { host.as_ref() }) else {
        return record_result(runtime, FAST_INVALID);
    };
    if !is_valid_edit_txn_ref(txn_ref) {
        return record_result(runtime, FAST_INVALID);
    }
    if !host.alive.load(Ordering::Acquire) {
        runtime.edit_txns.remove(&txn_ref);
        return record_result(runtime, FAST_INVALID);
    }
    let Some(txn) = runtime.edit_txns.remove(&txn_ref) else {
        return record_result(runtime, FAST_CACHE_MISS);
    };
    if txn.base_root_ref == 0 || txn.edits.is_empty() {
        return record_result(runtime, FAST_INVALID);
    }
    let trie = match runtime.build_edit_trie(&txn) {
        Ok(trie) => trie,
        Err(error) => return record_result(runtime, error),
    };
    let mut staged = Vec::with_capacity(trie.len());
    let root = match runtime.stage_edit_trie(txn.base_view, &trie, 0, &mut staged) {
        Ok(root) => root,
        Err(error) => return record_result(runtime, error),
    };
    if staged.is_empty() || staged.last().map(|(_, view)| view != &root).unwrap_or(true) {
        return record_result(runtime, FAST_INVALID);
    }
    let publication = match runtime.prepare_staged_publication(staged) {
        Ok(publication) => publication,
        Err(error) => return record_result(runtime, error),
    };
    if host.host.render(root).is_err() {
        return record_result(runtime, FAST_INTERNAL);
    }
    let result = runtime.commit_staged_publication(publication);
    record_result(runtime, result)
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn edit_txn_abort_impl(
    runtime: *mut NativeViewRuntime,
    txn_ref: u32,
) -> i32 {
    let Ok(runtime) = runtime_mut(runtime) else {
        return -1;
    };
    if !is_valid_edit_txn_ref(txn_ref) {
        return record_host_status(runtime, -1);
    }
    let status = if runtime.edit_txns.remove(&txn_ref).is_some() {
        0
    } else {
        1
    };
    record_host_status(runtime, status)
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
    let result = match runtime.publish(node_id, View::spacer(rows)) {
        Ok(reference) => reference,
        Err(error) => error,
    };
    record_result(runtime, result)
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
    let Ok((base_view, _)) = runtime.resolve_ref(base) else {
        return FAST_CACHE_MISS;
    };
    let Ok(patched) = base_view.try_with_text_layout_patch(Some(wrap), Some(align)) else {
        return FAST_INVALID;
    };
    let result = match runtime.publish(node_id, patched) {
        Ok(reference) => reference,
        Err(error) => error,
    };
    record_result(runtime, result)
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
    let Ok((base_view, _)) = runtime.resolve_ref(base) else {
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
    let result = match runtime.publish(node_id, patched) {
        Ok(reference) => reference,
        Err(error) => error,
    };
    record_result(runtime, result)
}

const AXIS_KIND_ROW: u32 = 1;
const AXIS_KIND_COLUMN: u32 = 2;
const MAX_AXIS_CHILD_COUNT: u32 = 524_288;

fn resolve_axis_children(
    runtime: &mut NativeViewRuntime,
    children: *const AxisChildInputV1,
    used_child_count: u32,
) -> Result<Vec<(u32, View)>, u32> {
    if used_child_count > MAX_AXIS_CHILD_COUNT {
        return Err(FAST_FALLBACK);
    }
    if used_child_count == 0 {
        return Ok(Vec::new());
    }
    if children.is_null() {
        return Err(FAST_INVALID);
    }
    let inputs = unsafe { std::slice::from_raw_parts(children, used_child_count as usize) };
    inputs
        .iter()
        .map(|input| {
            if input.track_word != 0 {
                let kind = input.track_word & 0xff;
                if !(1..=5).contains(&kind) {
                    return Err(FAST_INVALID);
                }
            }
            runtime
                .resolve_ref(input.child_ref)
                .map(|(view, _)| (input.track_word, view))
        })
        .collect()
}

fn publish_structural_path(
    runtime: &mut NativeViewRuntime,
    base_root_ref: u32,
    path_ref: u32,
    path_depth: u32,
    node_id_pairs: &[(u32, u32)],
    axis_index: Option<usize>,
    track_word: u32,
    grid_row: Option<usize>,
    grid_column: Option<usize>,
    child_ref: u32,
) -> u32 {
    if path_depth > 4 || node_id_pairs.len() != path_depth as usize + 1 {
        return FAST_INVALID;
    }
    if !is_valid_view_ref(base_root_ref) || !is_valid_view_ref(child_ref) {
        return FAST_INVALID;
    }
    let steps = match runtime.path_steps(path_ref) {
        Ok(steps) if steps.len() == path_depth as usize => steps,
        Ok(_) => return FAST_INVALID,
        Err(error) => return error,
    };
    let mut node_ids = Vec::with_capacity(node_id_pairs.len());
    for &(low, high) in node_id_pairs {
        let Ok(id) = node_id(low, high) else {
            return FAST_INVALID;
        };
        node_ids.push(id);
    }
    let Ok((base_view, _)) = runtime.resolve_ref(base_root_ref) else {
        return FAST_CACHE_MISS;
    };
    let Ok((child, _)) = runtime.resolve_ref(child_ref) else {
        return FAST_CACHE_MISS;
    };
    let Ok((root, views)) = base_view.native_replace_at_path(
        &steps,
        axis_index,
        track_word,
        grid_row,
        grid_column,
        child,
    ) else {
        return FAST_INVALID;
    };
    if views.len() != node_ids.len() || views.last() != Some(&root) {
        return FAST_INVALID;
    }
    if let Err(error) = validate_path_publication(runtime, &node_ids, &views) {
        return error;
    }
    let publication =
        match runtime.prepare_staged_publication(node_ids.into_iter().zip(views).collect()) {
            Ok(publication) => publication,
            Err(error) => return error,
        };
    runtime.commit_staged_publication(publication)
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn view_axis_create_buffer_impl(
    runtime: *mut NativeViewRuntime,
    node_id_low: u32,
    node_id_high: u32,
    axis_kind: u32,
    gap: u32,
    children: *const AxisChildInputV1,
    _children_capacity_bytes: usize,
    used_child_count: u32,
) -> u32 {
    let Ok(runtime) = runtime_mut(runtime) else {
        return FAST_INVALID;
    };
    let Ok(node_id) = node_id(node_id_low, node_id_high) else {
        return FAST_INVALID;
    };
    let Ok(gap) = u16::try_from(gap) else {
        return FAST_INVALID;
    };
    let children = match resolve_axis_children(runtime, children, used_child_count) {
        Ok(children) => children,
        Err(error) => return record_result(runtime, error),
    };
    let horizontal = match axis_kind {
        AXIS_KIND_ROW => true,
        AXIS_KIND_COLUMN => false,
        _ => return FAST_INVALID,
    };
    let Ok(view) = View::native_axis_from_children(horizontal, gap, children) else {
        return FAST_INVALID;
    };
    let result = runtime.publish(node_id, view).unwrap_or_else(|error| error);
    record_result(runtime, result)
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn view_axis_set_child_impl(
    runtime: *mut NativeViewRuntime,
    base_axis_ref: u32,
    node_id_low: u32,
    node_id_high: u32,
    child_index: u32,
    track_word: u32,
    child_ref: u32,
) -> u32 {
    let Ok(runtime) = runtime_mut(runtime) else {
        return FAST_INVALID;
    };
    let Ok(node_id) = node_id(node_id_low, node_id_high) else {
        return FAST_INVALID;
    };
    let Ok((base, _)) = runtime.resolve_ref(base_axis_ref) else {
        return FAST_CACHE_MISS;
    };
    let Ok((child, _)) = runtime.resolve_ref(child_ref) else {
        return FAST_CACHE_MISS;
    };
    let Ok(patched) = base.native_axis_set_child(child_index as usize, track_word, child) else {
        return FAST_INVALID;
    };
    let result = runtime
        .publish(node_id, patched)
        .unwrap_or_else(|error| error);
    record_result(runtime, result)
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn view_axis_splice_buffer_impl(
    runtime: *mut NativeViewRuntime,
    base_axis_ref: u32,
    node_id_low: u32,
    node_id_high: u32,
    index: u32,
    remove_count: u32,
    children: *const AxisChildInputV1,
    _children_capacity_bytes: usize,
    used_child_count: u32,
) -> u32 {
    let Ok(runtime) = runtime_mut(runtime) else {
        return FAST_INVALID;
    };
    let Ok(node_id) = node_id(node_id_low, node_id_high) else {
        return FAST_INVALID;
    };
    let Ok((base, _)) = runtime.resolve_ref(base_axis_ref) else {
        return FAST_CACHE_MISS;
    };
    let inserted = match resolve_axis_children(runtime, children, used_child_count) {
        Ok(inserted) => inserted,
        Err(error) => return record_result(runtime, error),
    };
    let Ok(patched) = base.native_axis_splice(index as usize, remove_count as usize, inserted)
    else {
        return FAST_INVALID;
    };
    let result = runtime
        .publish(node_id, patched)
        .unwrap_or_else(|error| error);
    record_result(runtime, result)
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn view_grid_set_cell_impl(
    runtime: *mut NativeViewRuntime,
    base_grid_ref: u32,
    node_id_low: u32,
    node_id_high: u32,
    row: u32,
    column: u32,
    child_ref: u32,
) -> u32 {
    let Ok(runtime) = runtime_mut(runtime) else {
        return FAST_INVALID;
    };
    let Ok(node_id) = node_id(node_id_low, node_id_high) else {
        return FAST_INVALID;
    };
    let Ok((base, _)) = runtime.resolve_ref(base_grid_ref) else {
        return FAST_CACHE_MISS;
    };
    let Ok((child, _)) = runtime.resolve_ref(child_ref) else {
        return FAST_CACHE_MISS;
    };
    let Ok(patched) = base.native_grid_set_cell(row as usize, column as usize, child) else {
        return FAST_INVALID;
    };
    let result = runtime
        .publish(node_id, patched)
        .unwrap_or_else(|error| error);
    record_result(runtime, result)
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn view_axis_set_child_path_impl(
    runtime: *mut NativeViewRuntime,
    base_root_ref: u32,
    path_ref: u32,
    path_depth: u32,
    target_node_id_low: u32,
    target_node_id_high: u32,
    ancestor0_node_id_low: u32,
    ancestor0_node_id_high: u32,
    ancestor1_node_id_low: u32,
    ancestor1_node_id_high: u32,
    ancestor2_node_id_low: u32,
    ancestor2_node_id_high: u32,
    ancestor3_node_id_low: u32,
    ancestor3_node_id_high: u32,
    axis_index: u32,
    track_word: u32,
    child_ref: u32,
) -> u32 {
    let Ok(runtime) = runtime_mut(runtime) else {
        return FAST_INVALID;
    };
    if path_depth > 4 {
        return record_result(runtime, FAST_INVALID);
    }
    let ids = [
        (target_node_id_low, target_node_id_high),
        (ancestor0_node_id_low, ancestor0_node_id_high),
        (ancestor1_node_id_low, ancestor1_node_id_high),
        (ancestor2_node_id_low, ancestor2_node_id_high),
        (ancestor3_node_id_low, ancestor3_node_id_high),
    ];
    let result = publish_structural_path(
        runtime,
        base_root_ref,
        path_ref,
        path_depth,
        &ids[..path_depth as usize + 1],
        Some(axis_index as usize),
        track_word,
        None,
        None,
        child_ref,
    );
    record_result(runtime, result)
}

#[unsafe(no_mangle)]
pub unsafe extern "Rust" fn view_grid_set_cell_path_impl(
    runtime: *mut NativeViewRuntime,
    base_root_ref: u32,
    path_ref: u32,
    path_depth: u32,
    target_node_id_low: u32,
    target_node_id_high: u32,
    ancestor0_node_id_low: u32,
    ancestor0_node_id_high: u32,
    ancestor1_node_id_low: u32,
    ancestor1_node_id_high: u32,
    ancestor2_node_id_low: u32,
    ancestor2_node_id_high: u32,
    ancestor3_node_id_low: u32,
    ancestor3_node_id_high: u32,
    grid_row: u32,
    grid_column: u32,
    child_ref: u32,
) -> u32 {
    let Ok(runtime) = runtime_mut(runtime) else {
        return FAST_INVALID;
    };
    if path_depth > 4 {
        return record_result(runtime, FAST_INVALID);
    }
    let ids = [
        (target_node_id_low, target_node_id_high),
        (ancestor0_node_id_low, ancestor0_node_id_high),
        (ancestor1_node_id_low, ancestor1_node_id_high),
        (ancestor2_node_id_low, ancestor2_node_id_high),
        (ancestor3_node_id_low, ancestor3_node_id_high),
    ];
    let result = publish_structural_path(
        runtime,
        base_root_ref,
        path_ref,
        path_depth,
        &ids[..path_depth as usize + 1],
        None,
        0,
        Some(grid_row as usize),
        Some(grid_column as usize),
        child_ref,
    );
    record_result(runtime, result)
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

fn reserve_staged_ref(
    slots: &HashMap<u32, NativeViewSlot>,
    planned: &mut HashSet<u32>,
    next: &mut u32,
) -> Result<u32, u32> {
    while *next < PATH_ROOT_REF {
        let candidate = *next;
        *next = (*next).saturating_add(1);
        if candidate != 0 && !slots.contains_key(&candidate) && planned.insert(candidate) {
            return Ok(candidate);
        }
    }
    Err(FAST_FALLBACK)
}

fn is_valid_view_ref(reference: u32) -> bool {
    (1..PATH_ROOT_REF).contains(&reference)
}

fn is_valid_path_ref(reference: u32) -> bool {
    (PATH_ROOT_REF..PATH_REF_LIMIT).contains(&reference)
}

fn is_valid_edit_txn_ref(reference: u32) -> bool {
    (EDIT_TXN_REF_START..EDIT_TXN_REF_LIMIT).contains(&reference)
}

fn set_trie_node_id(node: &mut EditTrieNode, node_id: u64) -> Result<(), u32> {
    match node.node_id {
        Some(existing) if existing != node_id => Err(FAST_INVALID),
        Some(_) => Ok(()),
        None => {
            node.node_id = Some(node_id);
            Ok(())
        }
    }
}

fn path_step_matches_kind(step_kind: u32, expected_view_kind: u32) -> bool {
    match step_kind {
        1 => expected_view_kind == 6,
        2 => expected_view_kind == 7,
        3 => expected_view_kind == 8,
        4 => expected_view_kind == 3,
        5 => expected_view_kind == 2,
        6 => expected_view_kind == 4,
        7..=9 => expected_view_kind == 5,
        _ => false,
    }
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
    use super::{
        AxisChildInputV1, FAST_CACHE_MISS, FAST_INVALID, MAX_EDIT_COUNT, NativeViewRuntime,
        PATH_ROOT_REF, generated_exports, is_valid_edit_txn_ref,
    };
    use iyon_tui::{GridTrack, IntoView, View};

    fn runtime() -> NativeViewRuntime {
        NativeViewRuntime::new()
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
            unsafe { generated_exports::iyon_view_release_many_v1(pointer, &reference, 4, 1) },
            1
        );
        assert_eq!(
            unsafe { generated_exports::iyon_view_render_ref_v1(pointer, reference) },
            FAST_CACHE_MISS
        );
    }

    #[test]
    fn bulk_publication_reuses_the_environment_native_ref_table() {
        let mut runtime = runtime();
        let view = View::spacer(3);
        let bulk_ref = runtime.publish_bulk(41, view.clone()).expect("bulk ref");
        assert_eq!(runtime.ref_for_node_id(41), Ok(bulk_ref));
        assert_eq!(runtime.resolve_ref(bulk_ref), Ok((view, true)));
    }

    #[test]
    fn repeated_node_id_lookups_acquire_independent_leases() {
        let mut runtime = runtime();
        let view = View::spacer(3);
        let reference = runtime.publish_bulk(41, view.clone()).expect("bulk ref");
        assert_eq!(runtime.ref_for_node_id(41), Ok(reference));
        assert_eq!(runtime.ref_for_node_id(41), Ok(reference));
        assert_eq!(
            runtime
                .slots
                .get(&reference)
                .map(|slot| slot.js_lease_count),
            Some(2)
        );

        assert_eq!(runtime.release_many(&reference, 1), Ok(1));
        assert_eq!(
            runtime
                .slots
                .get(&reference)
                .map(|slot| slot.js_lease_count),
            Some(1)
        );
        assert_eq!(runtime.release_many(&reference, 1), Ok(1));
        assert_eq!(
            runtime
                .slots
                .get(&reference)
                .map(|slot| slot.js_lease_count),
            Some(0)
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

    #[test]
    fn path_refs_are_interned_and_depth_specialization_rebuilds_only_the_path() {
        let mut runtime = runtime();
        let base_view = View::vertical(|column| {
            column.child(View::text("hello"));
        })
        .into_view();
        let base = runtime.publish(1, base_view).expect("base ref");
        let pointer = &mut runtime as *mut NativeViewRuntime;
        let root = unsafe { generated_exports::iyon_path_root_v1(pointer) };
        let path = unsafe { generated_exports::iyon_path_child_v1(pointer, root, 4, 3, 0) };
        assert_eq!(
            unsafe { generated_exports::iyon_path_child_v1(pointer, root, 4, 3, 0) },
            path
        );
        let patched = unsafe {
            generated_exports::iyon_view_text_layout_patch_path_d1_v1(
                pointer, base, path, 2, 0, 3, 0, 3, 2,
            )
        };
        assert!(patched < 0x8000_0000);
        assert_ne!(patched, base);
        assert!(
            runtime
                .nodes
                .get(&2)
                .and_then(iyon_tui::WeakView::upgrade)
                .is_some()
        );
        assert!(
            runtime
                .nodes
                .get(&3)
                .and_then(iyon_tui::WeakView::upgrade)
                .is_some()
        );
        assert!(runtime.resolve_ref(patched).is_ok());
        let generic = unsafe {
            generated_exports::iyon_view_text_layout_patch_path_v1(
                pointer, base, path, 1, 4, 0, 5, 0, 6, 0, 7, 0, 8, 0, 3, 2,
            )
        };
        assert!(generic < 0x8000_0000);
        assert!(
            runtime
                .nodes
                .get(&4)
                .and_then(iyon_tui::WeakView::upgrade)
                .is_some()
        );
        assert!(
            runtime
                .nodes
                .get(&5)
                .and_then(iyon_tui::WeakView::upgrade)
                .is_some()
        );
    }

    #[test]
    fn stale_path_base_returns_cache_miss_then_recovers_once() {
        let mut runtime = runtime();
        let base = runtime
            .publish(
                1,
                View::vertical(|column| {
                    column.child(View::text("hello"));
                })
                .into_view(),
            )
            .expect("base ref");
        let pointer = &mut runtime as *mut NativeViewRuntime;
        let root = unsafe { generated_exports::iyon_path_root_v1(pointer) };
        let path = unsafe { generated_exports::iyon_path_child_v1(pointer, root, 4, 3, 0) };
        assert_eq!(runtime.release_many(&base, 1), Ok(1));
        assert_eq!(
            unsafe {
                generated_exports::iyon_view_text_layout_patch_path_d1_v1(
                    pointer, base, path, 2, 0, 3, 0, 3, 2,
                )
            },
            FAST_CACHE_MISS
        );
        let recovered = runtime
            .publish(
                1,
                View::vertical(|column| {
                    column.child(View::text("hello"));
                })
                .into_view(),
            )
            .expect("recovered base ref");
        let patched = unsafe {
            generated_exports::iyon_view_text_layout_patch_path_d1_v1(
                pointer, recovered, path, 2, 0, 3, 0, 3, 2,
            )
        };
        assert!(patched < 0x8000_0000);
        assert!(runtime.nodes.contains_key(&2));
        assert!(runtime.nodes.contains_key(&3));
    }

    #[test]
    fn edit_transaction_builds_one_shared_ancestor_for_two_text_edits() {
        let mut runtime = runtime();
        let base_view = View::vertical(|column| {
            column.child(View::text("left"));
            column.child(View::text("right"));
        })
        .into_view();
        let base = runtime.publish(1, base_view).expect("base ref");
        let pointer = &mut runtime as *mut NativeViewRuntime;
        let path_root = unsafe { generated_exports::iyon_path_root_v1(pointer) };
        let path0 = unsafe { generated_exports::iyon_path_child_v1(pointer, path_root, 4, 3, 0) };
        let path1 = unsafe { generated_exports::iyon_path_child_v1(pointer, path_root, 4, 3, 1) };
        let txn = unsafe { generated_exports::iyon_edit_txn_begin_v1(pointer, base, 2) };
        assert!(is_valid_edit_txn_ref(txn));
        assert_eq!(
            unsafe {
                generated_exports::iyon_edit_txn_add_text_layout_v1(
                    pointer, txn, path0, 1, 11, 0, 21, 0, 21, 0, 21, 0, 21, 0, 3, 2,
                )
            },
            0
        );
        assert_eq!(
            unsafe {
                generated_exports::iyon_edit_txn_add_text_layout_v1(
                    pointer, txn, path1, 1, 12, 0, 21, 0, 21, 0, 21, 0, 21, 0, 3, 2,
                )
            },
            0
        );
        let transaction = runtime.edit_txns.get(&txn).expect("transaction");
        let trie = runtime.build_edit_trie(transaction).expect("trie");
        assert_eq!(trie.len(), 3, "root plus two changed leaves");
        let mut staged = Vec::new();
        let root = runtime
            .stage_edit_trie(transaction.base_view.clone(), &trie, 0, &mut staged)
            .expect("staged root");
        assert_eq!(staged.len(), 3, "shared root is rebuilt once");
        assert_eq!(staged.last().map(|(_, view)| view), Some(&root));
        assert!(staged.iter().any(|(id, _)| *id == 11));
        assert!(staged.iter().any(|(id, _)| *id == 12));
        assert_eq!(staged.last().map(|(id, _)| *id), Some(21));
    }

    #[test]
    fn edit_transaction_abort_and_limits_leave_no_staged_state() {
        let mut runtime = runtime();
        let base = runtime
            .publish(1, View::text("base").into_view())
            .expect("base ref");
        let pointer = &mut runtime as *mut NativeViewRuntime;
        assert_eq!(
            unsafe { generated_exports::iyon_edit_txn_begin_v1(pointer, base, 0) },
            FAST_INVALID
        );
        assert_eq!(
            unsafe { generated_exports::iyon_edit_txn_begin_v1(pointer, base, MAX_EDIT_COUNT + 1) },
            FAST_INVALID
        );
        let txn = unsafe { generated_exports::iyon_edit_txn_begin_v1(pointer, base, 1) };
        assert!(is_valid_edit_txn_ref(txn));
        assert_eq!(
            unsafe { generated_exports::iyon_edit_txn_abort_v1(pointer, txn) },
            0
        );
        assert!(!runtime.edit_txns.contains_key(&txn));
        assert_eq!(
            unsafe { generated_exports::iyon_edit_txn_abort_v1(pointer, txn) },
            1
        );
        assert_eq!(
            unsafe { generated_exports::iyon_edit_txn_begin_v1(pointer, PATH_ROOT_REF, 1) },
            FAST_INVALID
        );
    }

    #[test]
    fn generated_axis_and_grid_edits_copy_persistent_sequences() {
        let mut runtime = runtime();
        let axis = View::vertical(|column| {
            for index in 0..2_048 {
                column.child(View::text(format!("axis-{index}")));
            }
        })
        .into_view();
        let child = View::text("replacement").into_view();
        let base_axis = runtime.publish(1, axis.clone()).expect("axis base ref");
        let child_ref = runtime.publish(2, child.clone()).expect("child ref");
        let pointer = &mut runtime as *mut NativeViewRuntime;
        let replaced = unsafe {
            generated_exports::iyon_view_axis_set_child_v1(
                pointer, base_axis, 3, 0, 1_337, 0, child_ref,
            )
        };
        assert!(replaced < 0x8000_0000);
        assert_eq!(runtime.resolve_ref(base_axis), Ok((axis, true)));

        let inserted = [AxisChildInputV1 {
            track_word: 0,
            child_ref,
        }];
        let spliced = unsafe {
            generated_exports::iyon_view_axis_splice_buffer_v1(
                pointer,
                base_axis,
                4,
                0,
                1_000,
                0,
                inserted.as_ptr(),
                core::mem::size_of_val(&inserted),
                1,
            )
        };
        assert!(spliced < 0x8000_0000);

        let grid = View::grid(|grid| {
            grid.columns([GridTrack::fixed(12)]);
            grid.row(|row| {
                row.cell(View::text("grid-cell"));
            });
        })
        .into_view();
        let base_grid = runtime.publish(5, grid.clone()).expect("grid base ref");
        let grid_replaced = unsafe {
            generated_exports::iyon_view_grid_set_cell_v1(pointer, base_grid, 6, 0, 0, 0, child_ref)
        };
        assert!(grid_replaced < 0x8000_0000);
        assert_eq!(runtime.resolve_ref(base_grid), Ok((grid, true)));

        let path_grid_replaced = unsafe {
            generated_exports::iyon_view_grid_set_cell_path_v1(
                pointer,
                base_grid,
                PATH_ROOT_REF,
                0,
                8,
                0,
                1,
                0,
                1,
                0,
                1,
                0,
                1,
                0,
                0,
                0,
                child_ref,
            )
        };
        assert!(path_grid_replaced < 0x8000_0000);

        let path_root = unsafe { generated_exports::iyon_path_root_v1(pointer) };
        let path_replaced = unsafe {
            generated_exports::iyon_view_axis_set_child_path_v1(
                pointer, base_axis, path_root, 0, 7, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1_000, 0, child_ref,
            )
        };
        assert!(path_replaced < 0x8000_0000);
    }

    #[test]
    fn path_validation_rejects_wrong_parent_kind_and_preserves_publication() {
        let mut runtime = runtime();
        let base = runtime
            .publish(
                1,
                View::vertical(|column| {
                    column.child(View::text("hello"));
                })
                .into_view(),
            )
            .expect("base ref");
        let pointer = &mut runtime as *mut NativeViewRuntime;
        let root = unsafe { generated_exports::iyon_path_root_v1(pointer) };
        let path = unsafe { generated_exports::iyon_path_child_v1(pointer, root, 4, 3, 0) };
        let invalid = unsafe {
            generated_exports::iyon_view_text_layout_patch_path_d1_v1(
                pointer, base, path, 9, 0, 1, 0, 3, 2,
            )
        };
        assert!(invalid >= 0x8000_0000);
        assert!(!runtime.nodes.contains_key(&9));
        assert_eq!(
            unsafe { generated_exports::iyon_path_child_v1(pointer, root, 4, 1, 0) },
            FAST_INVALID
        );
        assert!(base < super::PATH_ROOT_REF);
        assert_eq!(
            unsafe { generated_exports::iyon_path_child_v1(pointer, base, 4, 3, 0) },
            FAST_INVALID
        );
        assert_eq!(
            unsafe {
                generated_exports::iyon_view_text_layout_patch_path_d1_v1(
                    pointer, base, base, 2, 0, 3, 0, 3, 2,
                )
            },
            FAST_INVALID
        );
        assert_eq!(
            runtime
                .status
                .code
                .load(std::sync::atomic::Ordering::Acquire),
            FAST_INVALID
        );
    }
}

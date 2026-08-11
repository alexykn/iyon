use std::{any::Any, sync::Arc};

use crate::{
    component::{ComponentId, ComponentRegistry, MountGraph},
    interaction::MountedCapabilities,
    presentation::layout::ComponentGeometryMap,
};

/// Host-owned semantic focus state.
pub(crate) struct FocusState {
    focused: Option<ComponentId>,
    focused_handler: Option<Arc<dyn Fn(&mut dyn Any, bool)>>,
    active_modal: Option<ComponentId>,
    modal_restore: Vec<(Option<ComponentId>, Option<ComponentId>)>,
    geometry: Option<ComponentGeometryMap>,
}

impl Default for FocusState {
    fn default() -> Self {
        Self {
            focused: None,
            focused_handler: None,
            active_modal: None,
            modal_restore: Vec::new(),
            geometry: None,
        }
    }
}

impl FocusState {
    pub(crate) fn focused(&self) -> Option<ComponentId> {
        self.focused
    }

    pub(crate) fn active_modal(&self) -> Option<ComponentId> {
        self.active_modal
    }

    #[cfg(test)]
    pub(crate) fn modal_restore_is_empty(&self) -> bool {
        self.modal_restore.is_empty()
    }

    pub(crate) fn reconcile_with_geometry(
        &mut self,
        graph: &MountGraph,
        capabilities: &MountedCapabilities,
        geometry: Option<&ComponentGeometryMap>,
        registry: &mut ComponentRegistry,
    ) -> bool {
        self.geometry = geometry.cloned();
        let previous_modal = self.active_modal;
        let next_modal = capabilities.modal_ids(graph.nodes.iter()).last();
        let modal_changed = next_modal != previous_modal;
        let restored = if modal_changed {
            let restore_index = self
                .modal_restore
                .iter()
                .rposition(|(target, _)| *target == next_modal);
            if let Some(index) = restore_index {
                self.modal_restore.truncate(index + 1);
                self.modal_restore.pop().and_then(|(_, focus)| focus)
            } else if next_modal.is_some() {
                self.prepare_new_modal(previous_modal, next_modal, graph, capabilities);
                None
            } else {
                // A non-nested transition that does not match a saved frame
                // must not leave entries for a later modal lifecycle.
                self.modal_restore.clear();
                None
            }
        } else {
            None
        };
        self.active_modal = next_modal;

        let order =
            eligible_focus_order_with_geometry(graph, capabilities, self.active_modal, geometry);
        let preferred = if modal_changed {
            restored
                .filter(|id| order.contains(id))
                .or_else(|| order.first().copied())
        } else if self.focused.is_some_and(|id| order.contains(&id)) {
            self.focused
        } else {
            order.first().copied()
        };

        self.set_focus(preferred, capabilities, registry)
    }

    fn prepare_new_modal(
        &mut self,
        previous_modal: Option<ComponentId>,
        next_modal: Option<ComponentId>,
        graph: &MountGraph,
        capabilities: &MountedCapabilities,
    ) {
        let Some(next_modal) = next_modal else {
            return;
        };
        let next_parent = modal_parent(next_modal, graph, capabilities);
        let previous_parent =
            previous_modal.and_then(|modal| modal_parent(modal, graph, capabilities));

        if next_parent == previous_modal {
            self.modal_restore.push((previous_modal, self.focused));
            return;
        }

        if next_parent == previous_parent {
            if self
                .modal_restore
                .last()
                .is_some_and(|(target, _)| *target == next_parent)
            {
                return;
            }
        }

        if let Some(index) = self
            .modal_restore
            .iter()
            .rposition(|(target, _)| *target == next_parent)
        {
            self.modal_restore.truncate(index + 1);
            return;
        }

        self.modal_restore.clear();
        self.modal_restore.push((next_parent, None));
    }

    pub(crate) fn focus_next(
        &mut self,
        graph: &MountGraph,
        capabilities: &MountedCapabilities,
        registry: &mut ComponentRegistry,
    ) -> bool {
        self.focus_step(graph, capabilities, registry, true)
    }

    pub(crate) fn focus_previous(
        &mut self,
        graph: &MountGraph,
        capabilities: &MountedCapabilities,
        registry: &mut ComponentRegistry,
    ) -> bool {
        self.focus_step(graph, capabilities, registry, false)
    }

    fn focus_step(
        &mut self,
        graph: &MountGraph,
        capabilities: &MountedCapabilities,
        registry: &mut ComponentRegistry,
        next: bool,
    ) -> bool {
        let order = eligible_focus_order_with_geometry(
            graph,
            capabilities,
            self.active_modal,
            self.geometry.as_ref(),
        );
        if order.is_empty() {
            return false;
        }

        let target = match self
            .focused
            .and_then(|focused| order.iter().position(|id| *id == focused))
        {
            Some(index) if next => order[(index + 1) % order.len()],
            Some(index) => order[(index + order.len() - 1) % order.len()],
            None if next => order[0],
            None => *order.last().expect("non-empty focus order"),
        };
        self.set_focus(Some(target), capabilities, registry);
        true
    }

    fn set_focus(
        &mut self,
        next: Option<ComponentId>,
        capabilities: &MountedCapabilities,
        registry: &mut ComponentRegistry,
    ) -> bool {
        if self.focused == next {
            self.focused_handler = next
                .and_then(|id| capabilities.get(id))
                .and_then(|caps| caps.focus_changed.as_ref())
                .cloned();
            return false;
        }

        let previous_handler = self.focused_handler.take();
        let previous = self.focused;
        self.focused = next;

        let mut changed = false;
        if let (Some(id), Some(handler)) = (previous, previous_handler) {
            notify_focus_handler(id, handler, false, registry);
            changed = true;
        }

        self.focused_handler = next
            .and_then(|id| capabilities.get(id))
            .and_then(|caps| caps.focus_changed.as_ref())
            .cloned();
        if let (Some(id), Some(handler)) = (next, self.focused_handler.as_ref()) {
            notify_focus_handler(id, handler.clone(), true, registry);
            changed = true;
        }
        changed
    }
}

fn modal_parent(
    modal: ComponentId,
    graph: &MountGraph,
    capabilities: &MountedCapabilities,
) -> Option<ComponentId> {
    let mut current = graph
        .nodes
        .iter()
        .find(|node| node.id == modal)
        .and_then(|node| node.parent);
    while let Some(id) = current {
        if capabilities.get(id).is_some_and(|caps| caps.modal_scope) {
            return Some(id);
        }
        current = graph
            .nodes
            .iter()
            .find(|node| node.id == id)
            .and_then(|node| node.parent);
    }
    None
}

pub(crate) fn eligible_focus_order_with_geometry(
    graph: &MountGraph,
    capabilities: &MountedCapabilities,
    modal: Option<ComponentId>,
    geometry: Option<&ComponentGeometryMap>,
) -> Vec<ComponentId> {
    graph
        .nodes
        .iter()
        .filter(|node| modal.is_none_or(|modal| is_descendant_or_self(node.id, modal, graph)))
        .filter(|node| capabilities.get(node.id).is_some_and(|caps| caps.focusable))
        .filter(|node| {
            geometry.is_none_or(|map| {
                map.entries
                    .get(&node.id)
                    .is_some_and(|item| item.visible.is_some())
            })
        })
        .map(|node| node.id)
        .collect()
}

pub(crate) fn is_descendant_or_self(
    id: ComponentId,
    ancestor: ComponentId,
    graph: &MountGraph,
) -> bool {
    let mut current = Some(id);
    while let Some(candidate) = current {
        if candidate == ancestor {
            return true;
        }
        current = graph
            .nodes
            .iter()
            .find(|node| node.id == candidate)
            .and_then(|node| node.parent);
    }
    false
}

fn notify_focus_handler(
    id: ComponentId,
    handler: Arc<dyn Fn(&mut dyn Any, bool)>,
    focused: bool,
    registry: &mut ComponentRegistry,
) {
    let _ = registry.with_any_mut(id, |component| handler(component, focused));
}

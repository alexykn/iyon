use crate::{
    component::{ComponentId, ComponentRegistry, MountGraph},
    interaction::MountedCapabilities,
};

/// Host-owned semantic focus state.
pub(crate) struct FocusState {
    focused: Option<ComponentId>,
    active_modal: Option<ComponentId>,
    modal_restore: Vec<(Option<ComponentId>, Option<ComponentId>)>,
}

impl Default for FocusState {
    fn default() -> Self {
        Self {
            focused: None,
            active_modal: None,
            modal_restore: Vec::new(),
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

    pub(crate) fn reconcile(
        &mut self,
        graph: &MountGraph,
        capabilities: &MountedCapabilities,
        registry: &mut ComponentRegistry,
    ) {
        let previous_modal = self.active_modal;
        let next_modal = capabilities.modal_ids(graph.nodes.iter()).last();
        let modal_changed = next_modal != previous_modal;
        let restored = if modal_changed
            && self
                .modal_restore
                .last()
                .is_some_and(|(target, _)| *target == next_modal)
        {
            self.modal_restore.pop().and_then(|(_, focus)| focus)
        } else if modal_changed && next_modal.is_some() {
            self.prepare_new_modal(previous_modal, next_modal, graph, capabilities);
            None
        } else if modal_changed {
            // A non-nested transition that does not match a saved frame must
            // not leave restoration entries for a later modal lifecycle.
            self.modal_restore.clear();
            None
        } else {
            None
        };
        self.active_modal = next_modal;

        let order = eligible_focus_order(graph, capabilities, self.active_modal);
        let preferred = if modal_changed {
            restored
                .filter(|id| order.contains(id))
                .or_else(|| order.first().copied())
        } else if self.focused.is_some_and(|id| order.contains(&id)) {
            self.focused
        } else {
            order.first().copied()
        };

        self.set_focus(preferred, capabilities, registry);
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
        let order = eligible_focus_order(graph, capabilities, self.active_modal);
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
    ) {
        if self.focused == next {
            return;
        }

        let previous = self.focused;
        self.focused = next;

        if let Some(id) = previous {
            notify_focus(id, false, capabilities, registry);
        }
        if let Some(id) = next {
            notify_focus(id, true, capabilities, registry);
        }
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

pub(crate) fn eligible_focus_order(
    graph: &MountGraph,
    capabilities: &MountedCapabilities,
    modal: Option<ComponentId>,
) -> Vec<ComponentId> {
    graph
        .nodes
        .iter()
        .filter(|node| modal.is_none_or(|modal| is_descendant_or_self(node.id, modal, graph)))
        .filter(|node| capabilities.get(node.id).is_some_and(|caps| caps.focusable))
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

fn notify_focus(
    id: ComponentId,
    focused: bool,
    capabilities: &MountedCapabilities,
    registry: &mut ComponentRegistry,
) {
    let Some(handler) = capabilities
        .get(id)
        .and_then(|caps| caps.focus_changed.as_ref())
        .cloned()
    else {
        return;
    };
    let _ = registry.with_any_mut(id, |component| handler(component, focused));
}

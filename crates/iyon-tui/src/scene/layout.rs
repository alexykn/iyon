use std::collections::HashMap;

use crate::{
    component::{ComponentId, ComponentRegistry, MountGraph},
    geometry::{LayoutConstraints, Size},
    interaction::MountedCapabilities,
    presentation::layout::{ComponentGeometryMap, LayoutTree, layout_view},
};

use super::ResolvedScene;

/// Ephemeral geometry derived from a resolved semantic scene.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResolvedSceneLayout {
    pub(crate) tree: LayoutTree,
    pub(crate) components: ComponentGeometryMap,
}

pub(crate) fn layout_resolved_scene(scene: &ResolvedScene, size: Size) -> ResolvedSceneLayout {
    let tree = layout_view(&scene.view, LayoutConstraints::bounded(size));
    let components = tree.component_geometry();
    ResolvedSceneLayout { tree, components }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LayoutSync {
    Stable,
    Dirty,
}

#[derive(Debug, Default)]
pub(crate) struct LayoutSynchronizer {
    delivered: HashMap<ComponentId, Size>,
}

impl LayoutSynchronizer {
    pub(crate) fn synchronize(
        &mut self,
        graph: &MountGraph,
        capabilities: &MountedCapabilities,
        geometry: &ComponentGeometryMap,
        registry: &mut ComponentRegistry,
    ) -> LayoutSync {
        self.delivered.retain(|id, _| graph.contains(*id));
        let mut dirty = false;
        for node in &graph.nodes {
            let Some(handler) = capabilities
                .get(node.id)
                .and_then(|caps| caps.layout_changed.as_ref())
                .cloned()
            else {
                self.delivered.remove(&node.id);
                continue;
            };
            let Some(entry) = geometry.entries.get(&node.id) else {
                unreachable!("mounted component has no layout geometry");
            };
            let size = entry.content.size();
            if self.delivered.get(&node.id).copied() == Some(size) {
                continue;
            }
            self.delivered.insert(node.id, size);
            registry.with_any_mut(node.id, |component| handler(component, size));
            dirty = true;
        }
        if dirty {
            LayoutSync::Dirty
        } else {
            LayoutSync::Stable
        }
    }
}

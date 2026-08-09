use crate::{component::MountGraph, interaction::MountedCapabilities, presentation::View};

/// A fully resolved semantic component scene, ready for presentation layout.
#[derive(Clone, Debug)]
pub(crate) struct ResolvedScene {
    pub(crate) view: View,
    pub(crate) mounts: MountGraph,
    pub(crate) capabilities: MountedCapabilities,
}

impl PartialEq for ResolvedScene {
    fn eq(&self, other: &Self) -> bool {
        self.view == other.view && self.mounts == other.mounts
    }
}

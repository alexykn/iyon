use crate::{component::MountGraph, presentation::View};

/// A fully resolved semantic component scene, ready for presentation layout.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ResolvedScene {
    pub(crate) view: View,
    pub(crate) mounts: MountGraph,
}

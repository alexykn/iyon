mod host;
mod layout;
mod resolve;
mod resolved;
mod root;

pub(crate) use host::{PreparedSceneFrame, SceneHost, SceneHostError};
pub(crate) use layout::{
    LayoutSync, LayoutSynchronizer, ResolvedSceneLayout, layout_resolved_scene,
};
#[cfg(test)]
pub(crate) use resolve::resolve_scene;
pub(crate) use resolve::{ResolveError, ResolveSession};
pub(crate) use resolved::ResolvedScene;
pub use root::Scene;
#[cfg(test)]
pub(crate) use root::resolve_root_scene;
pub(crate) use root::{ResolvedRootScene, resolve_root_scene_with_anchor};

#[cfg(test)]
mod root_tests;
#[cfg(test)]
mod tests;

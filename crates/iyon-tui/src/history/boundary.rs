//! Semantic attachment relationships between ordered history units.

/// Relationship between a history unit and its logical predecessor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FlowBoundary {
    /// Use the framework's default separation from the predecessor.
    #[default]
    Default,
    /// Semantically attach this unit to the predecessor.
    AttachToPrevious,
}

//! Semantic attachment relationships between ordered history units.

/// Relationship between a history unit and its logical predecessor.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FlowBoundary {
    /// Use the framework's default separation from the predecessor.
    #[default]
    Default,
    /// Semantically attach this unit to the predecessor.
    AttachToPrevious,
}

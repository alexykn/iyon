use crate::presentation::Insets;

/// Semantic layout configuration for one History flow.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct HistoryLayout {
    pub(crate) padding: Insets,
    pub(crate) gap: u16,
}

impl HistoryLayout {
    pub(crate) const fn new(padding: Insets, gap: u16) -> Self {
        Self { padding, gap }
    }
}

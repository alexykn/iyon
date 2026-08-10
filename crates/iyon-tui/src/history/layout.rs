use crate::presentation::Insets;

/// Semantic layout configuration for one History flow.
///
/// This describes reflowable resident presentation; native durability state
/// remains private to History.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HistoryLayout {
    pub(crate) padding: Insets,
    pub(crate) gap: u16,
}

impl HistoryLayout {
    pub const fn new(padding: Insets, gap: u16) -> Self {
        Self { padding, gap }
    }

    pub const fn padding(self) -> Insets {
        self.padding
    }

    pub const fn gap(self) -> u16 {
        self.gap
    }
}

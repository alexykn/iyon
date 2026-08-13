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
    /// Starts with default padding and gap; configure with fluent setters.
    pub const fn new() -> Self {
        Self {
            padding: Insets::ZERO,
            gap: 0,
        }
    }

    pub const fn from_parts(padding: Insets, gap: u16) -> Self {
        Self { padding, gap }
    }

    pub const fn with_padding(mut self, padding: Insets) -> Self {
        self.padding = padding;
        self
    }

    pub const fn with_gap(mut self, gap: u16) -> Self {
        self.gap = gap;
        self
    }

    pub const fn padding(self) -> Insets {
        self.padding
    }

    pub const fn gap(self) -> u16 {
        self.gap
    }
}

/// How a soft line break is presented as ordinary semantic text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SoftBreakPolicy {
    #[default]
    Space,
    LineBreak,
}

/// Structural-only policy for generic text-to-View lowering.
///
/// Semantic paint belongs to Theme. This type only controls document
/// structure such as block gap and soft-break presentation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextRenderPolicy {
    block_gap: u16,
    soft_break: SoftBreakPolicy,
}

impl Default for TextRenderPolicy {
    fn default() -> Self {
        Self {
            block_gap: 1,
            soft_break: SoftBreakPolicy::default(),
        }
    }
}

impl TextRenderPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn block_gap(&self) -> u16 {
        self.block_gap
    }

    pub fn with_block_gap(mut self, gap: u16) -> Self {
        self.block_gap = gap;
        self
    }

    pub fn soft_break(&self) -> SoftBreakPolicy {
        self.soft_break
    }

    pub fn with_soft_break(mut self, policy: SoftBreakPolicy) -> Self {
        self.soft_break = policy;
        self
    }
}

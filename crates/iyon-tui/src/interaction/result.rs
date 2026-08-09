/// Result of a generic component interaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractionResult {
    Ignored,
    Consumed,
}

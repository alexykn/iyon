use super::ComponentHandle;
use crate::presentation::ir::{
    ComponentSlotNode, Decoration, HeightRule, View, ViewKind, WidthRule,
};

impl View {
    /// Creates a live semantic mount for a retained component.
    pub fn component<C>(handle: ComponentHandle<C>) -> Self {
        Self {
            component: None,
            width: WidthRule::Fit,
            height: HeightRule::Fit,
            decoration: Decoration::default(),
            kind: ViewKind::ComponentSlot(ComponentSlotNode { id: handle.id() }),
        }
    }
}

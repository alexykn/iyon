use super::ComponentHandle;
use crate::presentation::ir::{
    ComponentSlotNode, Decoration, HeightRule, View, ViewKind, WidthRule,
};
use crate::presentation::{StyleFacts, StyleStates};

impl View {
    /// Creates a live semantic mount for a retained component.
    pub fn component<C>(handle: ComponentHandle<C>) -> Self {
        Self {
            component: None,
            width: WidthRule::Fit,
            height: HeightRule::Fit,
            decoration: Decoration::default(),
            style_states: StyleStates::default(),
            style_facts: StyleFacts::default(),
            component_scope: None,
            kind: ViewKind::ComponentSlot(ComponentSlotNode { id: handle.id() }),
        }
    }
}

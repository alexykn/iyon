use super::ComponentHandle;
use crate::component::ComponentId;
use crate::perf::{self, Counter};
use crate::presentation::ir::{
    ComponentSlotNode, Decoration, HeightRule, View, ViewKind, WidthRule,
};
use crate::presentation::{StyleFacts, StyleStates};

impl View {
    /// Creates a live semantic mount for a retained component.
    pub fn component<C>(handle: ComponentHandle<C>) -> Self {
        perf::inc(Counter::ViewNodesConstructedRust);
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

    /// Creates a live component slot from a native host component identity.
    pub fn native_component(raw_id: u64) -> Self {
        assert!(raw_id != 0, "native component identity must be non-zero");
        perf::inc(Counter::ViewNodesConstructedRust);
        Self {
            component: None,
            width: WidthRule::Fit,
            height: HeightRule::Fit,
            decoration: Decoration::default(),
            style_states: StyleStates::default(),
            style_facts: StyleFacts::default(),
            component_scope: None,
            kind: ViewKind::ComponentSlot(crate::presentation::ir::ComponentSlotNode {
                id: ComponentId::from_raw(raw_id),
            }),
        }
    }
}

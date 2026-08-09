use super::ComponentHandle;
use crate::presentation::ir::{ComponentSlotNode, Decoration, View, ViewKind, WidthRule};

impl View {
    /// Creates a private semantic placement for a retained component.
    pub(crate) fn component_slot<C>(handle: ComponentHandle<C>) -> Self {
        Self {
            component: None,
            width: WidthRule::Fit,
            decoration: Decoration::default(),
            kind: ViewKind::ComponentSlot(ComponentSlotNode { id: handle.id() }),
        }
    }
}

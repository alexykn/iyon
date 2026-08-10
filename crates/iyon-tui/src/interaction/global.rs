use crate::output::EventCx;

use super::{InteractionResult, KeyStroke};

pub(crate) type GlobalBinding = for<'a> fn(KeyStroke, &mut EventCx<'a>) -> InteractionResult;

#[derive(Default)]
pub(crate) struct GlobalBindings {
    bindings: Vec<GlobalBinding>,
}

impl GlobalBindings {
    /// S9 seam: AppCx will populate application-owned global actions.
    pub(crate) fn push(&mut self, binding: GlobalBinding) {
        self.bindings.push(binding);
    }

    pub(crate) fn dispatch(&self, key: KeyStroke, cx: &mut EventCx<'_>) -> InteractionResult {
        for binding in &self.bindings {
            if matches!(binding(key, cx), InteractionResult::Consumed) {
                return InteractionResult::Consumed;
            }
        }
        InteractionResult::Ignored
    }
}

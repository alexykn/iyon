//! Application panel Components.
//!
//! Panels own semantic state and rendering. Placement, sizing, focus, and
//! visibility are decided by the containing Scene View and generic host.

use iyon_tui::{Component, View};

#[derive(Debug, Default)]
pub(crate) struct SteeringQueuePanel {
    pending: Vec<String>,
}

impl SteeringQueuePanel {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn queued(&mut self, text: String) {
        self.pending.push(text);
    }

    pub(crate) fn delivered(&mut self, text: &str) {
        if let Some(index) = self.pending.iter().position(|item| item == text) {
            self.pending.remove(index);
        }
    }

    pub(crate) fn pending(&self) -> Vec<String> {
        self.pending.clone()
    }
}

impl Component for SteeringQueuePanel {
    fn view(&self) -> View {
        View::spacer(0)
    }
}

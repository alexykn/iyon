//! Application panel Components.
//!
//! Panels own semantic state and rendering. Placement, sizing, focus, and
//! visibility are decided by the containing Scene View and generic host.

use crate::{Component, View};

const MAX_PENDING_ROWS: usize = 4;

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
}

impl Component for SteeringQueuePanel {
    fn view(&self) -> View {
        if self.pending.is_empty() {
            return View::spacer(0);
        }

        let mut rows = self
            .pending
            .iter()
            .take(MAX_PENDING_ROWS)
            .map(|text| View::text(format!("Steering: {text}")).fill_width())
            .collect::<Vec<_>>();
        let overflow = self.pending.len().saturating_sub(MAX_PENDING_ROWS);
        if overflow > 0 {
            rows.push(View::text(format!("↑ {overflow} more queued")).fill_width());
        }
        View::vertical(|column| {
            column.children(rows);
        })
        .fill_width()
    }
}

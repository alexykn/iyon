//! Application panel Components.
//!
//! Panels own semantic state and rendering. Placement, sizing, focus, and
//! visibility are decided by the containing Scene View and generic host.

use iyon_tui::{ColorSpec, Component, IntoView, View};

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

pub(crate) fn status_row(spinner: &str, label: &str, pending: &[String]) -> View {
    let status = View::text(format!("{spinner} {label}")).no_wrap();
    let row = match queue_parts(pending) {
        Some((preview, extra)) => View::horizontal(|row| {
            row.gap(4);
            row.child(status);
            row.flex(queue_text(preview));
            if let Some(extra) = extra {
                row.child(queue_text(extra));
            }
        }),
        None => status.into_view(),
    };
    row.fill_width().padding(crate::tui::viewport_gutter())
}

fn queue_parts(pending: &[String]) -> Option<(String, Option<String>)> {
    let first = pending.first()?;
    let preview = first.split_whitespace().collect::<Vec<_>>().join(" ");
    let extra = pending.len().saturating_sub(1);
    let extra = (extra > 0).then(|| format!(" + {extra} more"));
    Some((format!("Queue: {preview}"), extra))
}

fn queue_text(text: String) -> View {
    View::text(text)
        .no_wrap()
        .italic()
        .foreground(ColorSpec::theme("text.muted"))
        .into_view()
}

use iyon_tui::{FlowBoundary, History, HistoryLayout, Insets};

fn main() {
    let mut history = History::new();
    history.set_layout(HistoryLayout::new(Insets::ZERO, 1));
    history.push("A").unwrap();
    history
        .push_with_boundary("B", FlowBoundary::AttachToPrevious)
        .unwrap();
}

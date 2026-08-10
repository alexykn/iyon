use iyon_tui::{History, View};

fn main() {
    let history = History::new();
    let _ = View::vertical(|column| {
        column.child(history);
    });
}

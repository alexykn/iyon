use iyon_tui::{History, IntoView};

fn accepts_view(_: impl IntoView) {}

fn main() {
    accepts_view(History::new());
}

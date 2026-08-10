use iyon_tui::{History, Scene};

fn main() {
    let _ = Scene::new("body");
    let _ = Scene::with_history(History::new(), "body");
}

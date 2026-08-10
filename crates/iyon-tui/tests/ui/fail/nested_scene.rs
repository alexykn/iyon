use iyon_tui::{History, Scene};

fn main() {
    let inner = Scene::with_history(History::new(), "inner body");
    let _ = Scene::with_history(History::new(), inner);
}

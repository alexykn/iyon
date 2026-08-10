use iyon_tui::{IntoView, Scene};

fn accepts_view(_: impl IntoView) {}

fn main() {
    accepts_view(Scene::new("body"));
}

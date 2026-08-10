use iyon_tui::{History, IntoView, Scene, View};

#[test]
fn body_only_scene_has_no_history() {
    let mut scene = Scene::new("body");
    assert!(scene.history().is_none());
    assert_eq!(scene.body(), &"body".into_view());
    scene.set_body(View::text("changed"));
    assert_eq!(scene.body(), &"changed".into_view());
}

#[test]
fn scene_can_own_one_history_and_mutate_it() {
    let mut scene = Scene::with_history(History::new(), "body");
    assert!(scene.history().is_some());
    scene.history_mut().unwrap().push("earlier").unwrap();
    assert_eq!(scene.body(), &"body".into_view());
}

fn main() {
    fn assert_visitor<T: iyon_tui::TextVisitor>() {}
    assert_visitor::<()>();
}

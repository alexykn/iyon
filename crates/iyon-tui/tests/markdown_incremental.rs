use iyon_tui::*;
fn input(s: &str, stable: usize, sealed: bool) -> Projection<TextContent> {
    let b = StreamOffset::ZERO;
    let e = StreamOffset::new(s.len() as u64);
    let st = StreamOffset::new(stable as u64);
    ProjectionBuilder::new(b, st, e, sealed)
        .emit(StreamRange::new(b, e), TextContent::raw(s))
        .finish()
        .unwrap()
}
fn kinds(p: &Projection<TextContent>) -> Vec<String> {
    p.spans()
        .iter()
        .flat_map(|s| s.values())
        .filter_map(|v| match v {
            TextContent::Block(b) => Some(format!("{:?}", b.kind())),
            _ => None,
        })
        .collect()
}
#[test]
fn setext_stays_mutable_until_seal() {
    let mut m = MarkdownProjector::default();
    let a = m.project(&input("Foo\n", 4, false)).unwrap();
    assert!(a.stable_through().as_u64() < 4);
    let b = m.project(&input("Foo\n---\n", 8, false)).unwrap();
    assert!(kinds(&b).iter().any(|k| k.contains("Heading")));
    let c = m.project(&input("Foo\n---\n", 8, true)).unwrap();
    assert_eq!(c.stable_through(), c.source_end());
}
#[test]
fn references_are_parsed_and_incremental_seal_converges() {
    let source = "[foo]\n\n[foo]: /url\n";
    let mut m = MarkdownProjector::default();
    let first = m.project(&input("[foo]\n\n", 7, false)).unwrap();
    assert!(first.stable_through().as_u64() < 7);
    let final_one = m.project(&input(source, source.len(), true)).unwrap();
    let mut fresh = MarkdownProjector::default();
    let batch = fresh.project(&input(source, source.len(), true)).unwrap();
    assert_eq!(final_one, batch);
}

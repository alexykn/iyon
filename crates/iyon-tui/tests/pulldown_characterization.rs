use pulldown_cmark::{Alignment, Event, Options, Parser, Tag, TagEnd};

fn events(source: &str) -> Vec<(Event<'_>, std::ops::Range<usize>)> {
    Parser::new_ext(source, Options::all())
        .into_offset_iter()
        .collect()
}

#[test]
fn representative_events_keep_source_bounded_ranges() {
    let cases = [
        ("plain", "plain"),
        ("escaped", r#"\\*literal* &amp;"#),
        ("emphasis", "*em* **strong** ~~strike~~"),
        ("inline code", "`code`"),
        ("links", "[inline](url) [ref][id]\n\n[id]: /target\n"),
        ("atx", "# heading\n"),
        ("setext", "heading\n=======\n"),
        ("fence", "```rust\nfn main() {}\n```\n"),
        ("indented", "    code\n"),
        ("quote", "> quote\n"),
        ("tight list", "- a\n- b\n"),
        ("loose list", "- a\n\n- b\n"),
        ("ordered", "1) a\n2) b\n"),
        ("task", "- [x] done\n"),
        ("html", "<span>inline</span>\n\n<div>block</div>\n"),
        ("table", "| a | b |\n| :- | -: |\n| c | d |\n"),
        ("breaks", "soft\nbreak\n\nhard  \nbreak\n"),
        ("rule", "---\n"),
    ];

    for (name, source) in cases {
        for (_, range) in events(source) {
            assert!(range.start <= range.end, "{name}: invalid range {range:?}");
            assert!(range.end <= source.len(), "{name}: out of bounds {range:?}");
            assert!(source.is_char_boundary(range.start));
            assert!(source.is_char_boundary(range.end));
        }
    }
}

#[test]
fn event_shapes_lock_the_adapter_contract() {
    let source =
        "# h\n\n- [x] a\n\n| a | b |\n| :- | -: |\n| c | d |\n\n```json\n{\"ok\":true}\n```\n";
    let parsed = events(source);
    assert!(
        parsed
            .iter()
            .any(|(event, _)| matches!(event, Event::Start(Tag::Heading { .. })))
    );
    assert!(
        parsed
            .iter()
            .any(|(event, _)| matches!(event, Event::Start(Tag::List(_))))
    );
    assert!(
        parsed
            .iter()
            .any(|(event, _)| matches!(event, Event::TaskListMarker(true)))
    );
    assert!(
        parsed
            .iter()
            .any(|(event, _)| matches!(event, Event::Start(Tag::Table(_))))
    );
    assert!(
        parsed
            .iter()
            .any(|(event, _)| matches!(event, Event::Start(Tag::CodeBlock(_))))
    );
    assert!(
        parsed
            .iter()
            .any(|(event, _)| matches!(event, Event::End(TagEnd::CodeBlock)))
    );
}

#[test]
fn alignment_and_transformed_text_are_observable_at_the_event_boundary() {
    let table = events("| a | b |\n| :- | -: |\n| c | d |\n");
    assert!(table.iter().any(|(event, _)| {
        matches!(event, Event::Start(Tag::Table(columns)) if columns.as_slice() == [Alignment::Left, Alignment::Right])
    }));

    let transformed = events(r#"\\*literal* &amp;"#);
    assert!(transformed.iter().any(|(event, range)| {
        matches!(event, Event::Text(text) if text.as_ref() != &r#"\\*literal* &amp;"#[..] && range.start < range.end)
    }));
}

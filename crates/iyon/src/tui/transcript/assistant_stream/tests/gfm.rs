use super::*;
use crate::tui::theme::iyon_theme;
use iyon_tui::text::{BlockKind, Mark, TextContent, TextOrigin, TextProvenance, walk_content};
use iyon_tui::{Block, View};

const GFM_FIXTURE: &str =
    "| Item | State |\n| --- | --- |\n| ~~old~~ | active |\n\n- [x] complete\n- [ ] pending\n";

const PARITY_FIXTURE: &str = r###"# Result

| Name | Value |
| --- | --- |
| alpha | ~~old~~ |
| beta | new |

- [x] done
- [ ] pending

> quoted **strong**

```rust
fn main() {}
```
"###;

const MIGRATION_FIXTURE: &str = r###"# Migration result

The old path is ~~deprecated~~.

| File | Status | Notes |
| --- | :---: | --- |
| api.rs | done | shared renderer |
| ui.rs | pending | needs review |

- [x] parsing
- [x] rendering
- [ ] cleanup

> The old configuration remains compatible.

```rust
fn migrate() {
    todo!();
}
```

---
"###;

fn sealed_text(source: &str) -> AssistantStream {
    let mut stream = stream_with(&[(SegmentKind::Text, source)]);
    stream.seal();
    stream
}

fn blocks(stream: &AssistantStream) -> Vec<&Block> {
    stream
        .semantic
        .spans()
        .iter()
        .flat_map(|span| span.values())
        .filter_map(|value| match value {
            TextContent::Block(block) => Some(block),
            TextContent::Raw(_) => None,
        })
        .collect()
}

fn table_of(stream: &AssistantStream) -> &iyon_tui::text::Table {
    blocks(stream)
        .into_iter()
        .find_map(|block| match block.kind() {
            BlockKind::Table(table) => Some(table),
            _ => None,
        })
        .expect("expected table")
}

fn lists_of(stream: &AssistantStream) -> Vec<&iyon_tui::text::List> {
    blocks(stream)
        .into_iter()
        .filter_map(|block| block.as_list())
        .collect()
}

fn painted(view: &View, width: u16) -> Vec<String> {
    iyon_tui::testing::compile_view_lines(view, width)
}

fn painted_joined(view: &View, width: u16) -> String {
    painted(view, width).join("\n")
}

fn find_x(lines: &[String], needle: &str) -> usize {
    lines
        .iter()
        .find_map(|line| line.find(needle))
        .unwrap_or_else(|| panic!("did not find {needle:?} in {lines:?}"))
}

fn collect_runs(stream: &AssistantStream) -> Runs {
    let mut runs = Runs::default();
    for span in stream.semantic.spans() {
        for value in span.values() {
            walk_content(&mut runs, value);
        }
    }
    runs
}

fn concat_marked(stream: &AssistantStream, pred: impl Fn(&[Mark], bool) -> bool) -> String {
    collect_runs(stream)
        .marked
        .iter()
        .filter(|(_, marks, thinking)| pred(marks, *thinking))
        .map(|(text, _, _)| text.as_str())
        .collect()
}

fn has_strike(stream: &AssistantStream, text: &str) -> bool {
    concat_marked(stream, |marks, _| marks.contains(&Mark::Strikethrough)).contains(text)
}

fn exact_source_slice<'a>(source: &'a str, provenance: &TextProvenance) -> Option<&'a str> {
    match provenance {
        TextProvenance::Exact(range) => {
            let start = usize::try_from(range.start().as_u64()).ok()?;
            let end = usize::try_from(range.end().as_u64()).ok()?;
            source.get(start..end)
        }
        _ => None,
    }
}

fn advance_some(source: &mut AssistantStream, steps: usize) {
    let mut now = Instant::now();
    for _ in 0..steps {
        now += Duration::from_millis(16);
        if !source.advance(now) && source.next_wakeup().is_none() {
            break;
        }
    }
}

#[test]
fn assistant_pipeline_projects_gfm_semantics() {
    let stream = sealed_text(GFM_FIXTURE);
    let table = table_of(&stream);
    assert_eq!(table.header_rows(), 1);
    assert_eq!(
        blocks(&stream)
            .iter()
            .find(|block| matches!(block.kind(), BlockKind::Table(_)))
            .and_then(|block| block.origin()),
        Some(TextOrigin::MARKDOWN)
    );
    assert!(has_strike(&stream, "old"));

    let list = lists_of(&stream).into_iter().next().expect("task list");
    assert_eq!(list.items()[0].checked(), Some(true));
    assert_eq!(list.items()[1].checked(), Some(false));
    assert_eq!(list.items()[0].origin(), Some(TextOrigin::MARKDOWN));
}

#[test]
fn assistant_product_render_uses_gfm_and_task_only() {
    let stream = sealed_text(GFM_FIXTURE);
    let view = assistant_view(&stream.semantic, &assistant_renderer());
    let theme = iyon_theme();
    let lines = painted(&view, 40);
    let joined = lines.join("\n");

    assert!(
        iyon_tui::testing::style_at_text(&view, 40, &theme, "Item").bold,
        "table header remains Theme/framework bold"
    );
    assert!(iyon_tui::testing::style_at_text(&view, 40, &theme, "old").strikethrough);
    assert!(joined.contains("[x] complete"), "{joined}");
    assert!(
        !joined.contains("[x] - complete") && !joined.contains("[x]- complete"),
        "TaskOnly must not render the list bullet: {joined}"
    );
    assert_eq!(find_x(&lines, "Item"), find_x(&lines, "old"));

    let code = sealed_text("```rust\nfn main() {}\n```\n");
    let code_view = assistant_view(&code.semantic, &assistant_renderer());
    let label = iyon_tui::testing::style_at_text(&code_view, 40, &theme, "rust");
    assert_eq!(label.fg_rgb, Some((113, 128, 150)));
    assert!(label.dim);
    assert_eq!(
        iyon_tui::testing::style_at_text(&code_view, 40, &theme, "fn").fg_rgb,
        Some((120, 200, 210))
    );
}

#[test]
fn gfm_snapshot_and_final_assistant_view_match_across_widths() {
    let renderer = assistant_renderer();
    for width in [1, 2, 4, 8, 12, 20, 40, 80] {
        let stream = sealed_text(PARITY_FIXTURE);
        let stream_view = stream.snapshot().view_for_test();
        let final_view = assistant_view(&stream.semantic, &renderer);
        assert_eq!(
            painted(&stream_view, width),
            painted(&final_view, width),
            "physical mismatch at width {width}"
        );
        if width >= 20 {
            let theme = iyon_theme();
            assert_eq!(
                iyon_tui::testing::style_at_text(&stream_view, width, &theme, "Result"),
                iyon_tui::testing::style_at_text(&final_view, width, &theme, "Result"),
            );
            assert_eq!(
                iyon_tui::testing::style_at_text(&stream_view, width, &theme, "old"),
                iyon_tui::testing::style_at_text(&final_view, width, &theme, "old"),
            );
        }
    }
}

#[test]
fn live_gfm_table_evolution_converges_with_one_shot() {
    let stages = ["| A", " | B", " |\n", "|---", "|---|\n", "| 1", " | 2 |"];
    let full: String = stages.concat();
    let mut history = History::new();
    let handle = history.push_stream(AssistantStream::new()).unwrap();
    let mut previous_stable = StreamOffset::ZERO;
    for stage in stages {
        history
            .update_stream(handle, |source| {
                source.push_delta_paced(SegmentKind::Text, stage);
                advance_some(source, 8);
                let snapshot = source.snapshot();
                assert!(snapshot.source_base() <= snapshot.stable_through());
                assert!(snapshot.stable_through() <= snapshot.source_end());
                assert!(snapshot.source_end() <= source.received_end);
                assert!(snapshot.stable_through() >= previous_stable);
                previous_stable = snapshot.stable_through();
            })
            .expect("live GFM table evolution must not fail History");
    }
    let (live_semantic, live_view) = history
        .update_stream(handle, |source| {
            source.seal();
            (source.semantic.clone(), source.snapshot().view_for_test())
        })
        .unwrap();
    history.seal_stream(handle).unwrap();

    let mut whole = stream_with(&[(SegmentKind::Text, &full)]);
    whole.seal();
    assert_eq!(live_semantic, whole.semantic);
    let whole_view = whole.snapshot().view_for_test();
    for width in [8, 20, 40, 80] {
        assert_eq!(
            painted(&live_view, width),
            painted(&whole_view, width),
            "final physical mismatch at width {width}"
        );
    }
}

#[test]
fn split_strikethrough_delimiter_seals_without_stale_markers() {
    let mut stream = stream_with(&[
        (SegmentKind::Text, "before ~~ol"),
        (SegmentKind::Text, "d~~ after"),
    ]);
    stream.seal();
    assert!(has_strike(&stream, "old"));
    let view = assistant_view(&stream.semantic, &assistant_renderer());
    let theme = iyon_theme();
    assert!(iyon_tui::testing::style_at_text(&view, 40, &theme, "old").strikethrough);
    let joined = painted_joined(&view, 40);
    assert!(
        !joined.contains("~~"),
        "no leftover strike delimiters: {joined}"
    );
    assert!(!joined.contains("before ~~") && !joined.contains("~~ after"));
}

#[test]
fn split_task_marker_seals_as_task_only() {
    let mut stream = stream_with(&[
        (SegmentKind::Text, "- ["),
        (SegmentKind::Text, "x] complete"),
    ]);
    stream.seal();
    let list = lists_of(&stream).into_iter().next().expect("task list");
    assert_eq!(list.items()[0].checked(), Some(true));
    let joined = painted_joined(&assistant_view(&stream.semantic, &assistant_renderer()), 40);
    assert!(joined.contains("[x] complete"), "{joined}");
    assert!(!joined.contains("[x] -"), "{joined}");
}

#[test]
fn table_delimiter_split_never_revises_stable_prefix() {
    let mut stream = AssistantStream::new();
    stream.push_delta_paced(SegmentKind::Text, "| A | B |\n");
    drain_pacing(&mut stream);
    let after_header = stream.snapshot();
    assert!(after_header.stable_through() <= after_header.source_end());
    let stable = after_header.stable_through();

    stream.push_delta_paced(SegmentKind::Text, "| --- | --- |\n");
    drain_pacing(&mut stream);
    let after_delimiter = stream.snapshot();
    assert!(after_delimiter.stable_through() >= stable);
    stream.seal();
    assert!(
        table_of(&stream).header_rows() >= 1 || !table_of(&stream).rows().is_empty(),
        "delimiter row must produce a table"
    );
}

#[test]
fn live_history_compaction_survives_grid_backed_tables() {
    let mut history = History::new();
    let handle = history.push_stream(AssistantStream::new()).unwrap();
    history
        .update_stream(handle, |source| {
            source.push_delta_paced(SegmentKind::Text, "stable paragraph\n\n");
            drain_pacing(source);
        })
        .unwrap();
    history
        .update_stream(handle, |source| {
            source.push_delta_paced(
                SegmentKind::Text,
                "| Name | Value |\n| --- | --- |\n| a | b |\n\n",
            );
            drain_pacing(source);
        })
        .expect("table stage must survive live compaction");
    history
        .update_stream(handle, |source| {
            source.push_delta_paced(SegmentKind::Text, "- [x] done\n\n");
            drain_pacing(source);
        })
        .expect("task list stage must survive live compaction");
    history
        .update_stream(handle, |source| {
            source.push_delta_paced(SegmentKind::Text, "later paragraph");
        })
        .expect("later paragraph must keep unpublished suffix");
    history
        .seal_stream(handle)
        .expect("seal after structured compaction must succeed");
}

#[test]
fn sealed_history_gfm_matches_direct_assistant_view() {
    let mut history = History::new();
    let handle = history.push_stream(AssistantStream::new()).unwrap();
    history
        .update_stream(handle, |source| {
            source.push_delta_paced(SegmentKind::Text, PARITY_FIXTURE);
            drain_pacing(source);
        })
        .unwrap();
    let renderer = assistant_renderer();
    let (history_view, direct) = history
        .update_stream(handle, |source| {
            source.seal();
            (
                source.snapshot().view_for_test(),
                assistant_view(&source.semantic, &renderer),
            )
        })
        .unwrap();
    history.seal_stream(handle).unwrap();
    for width in [8, 20, 40, 80] {
        assert_eq!(
            painted(&history_view, width),
            painted(&direct, width),
            "History vs assistant_view mismatch at width {width}"
        );
    }
}

#[test]
fn thinking_gfm_keeps_structure_and_run_annotations() {
    let source = "| Step | Why |\n| --- | --- |\n| one | **reason** |\n\n- [x] considered\n";
    let mut stream = stream_with(&[(SegmentKind::Thinking, source)]);
    stream.seal();
    let table = table_of(&stream);
    assert_eq!(table.header_rows(), 1);
    let list = lists_of(&stream).into_iter().next().expect("task list");
    assert_eq!(list.items()[0].checked(), Some(true));

    assert!(
        concat_marked(&stream, |marks, thinking| {
            marks.contains(&Mark::Strong) && thinking
        })
        .contains("reason")
    );
    assert!(concat_marked(&stream, |_, thinking| thinking).contains("one"));
    assert!(concat_marked(&stream, |_, thinking| thinking).contains("considered"));

    let view = assistant_view(&stream.semantic, &assistant_renderer());
    let muted = iyon_tui::testing::style_at_text(&view, 40, &iyon_theme(), "reason");
    assert_eq!(muted.fg_rgb, Some((113, 128, 150)));
    assert!(muted.italic);
}

#[test]
fn thinking_to_answer_gfm_boundary_does_not_bleed() {
    let thinking = "| Step | Why |\n| --- | --- |\n| one | two |";
    let answer = "| File | Status |\n| --- | --- |\n| api.rs | done |\n";
    let mut stream = stream_with(&[
        (SegmentKind::Thinking, thinking),
        (SegmentKind::Text, answer),
    ]);
    stream.seal();
    let tables: Vec<_> = blocks(&stream)
        .into_iter()
        .filter(|block| matches!(block.kind(), BlockKind::Table(_)))
        .collect();
    assert_eq!(tables.len(), 2);

    assert!(concat_marked(&stream, |_, thinking| thinking).contains("one"));
    assert!(concat_marked(&stream, |_, thinking| !thinking).contains("api.rs"));
    assert!(!concat_marked(&stream, |_, thinking| thinking).contains("api.rs"));
    assert_eq!(
        stream.received_end.as_u64(),
        format!("{thinking}\n\n{answer}").len() as u64
    );
}

#[test]
fn thinking_code_label_is_generated_chrome() {
    let mut stream = stream_with(&[(SegmentKind::Thinking, "```rust\nfn main() {}\n```\n")]);
    stream.seal();
    let runs = collect_runs(&stream);
    assert!(
        runs.thinking.concat().contains("fn"),
        "code body runs keep thinking annotations: {:?}",
        runs.thinking
    );
    let view = assistant_view(&stream.semantic, &assistant_renderer());
    let joined = painted_joined(&view, 40);
    assert!(joined.contains("rust"), "{joined}");
    assert!(joined.contains("fn main"), "{joined}");
}

#[test]
fn gfm_product_fixture_renders_through_assistant_stream() {
    let stream = sealed_text(MIGRATION_FIXTURE);
    let view = assistant_view(&stream.semantic, &assistant_renderer());
    let theme = iyon_theme();
    let joined = painted_joined(&view, 80);
    for needle in [
        "Migration result",
        "deprecated",
        "api.rs",
        "ui.rs",
        "parsing",
        "rendering",
        "cleanup",
        "old configuration",
        "rust",
        "todo!",
        "───",
    ] {
        assert!(joined.contains(needle), "missing {needle:?} in {joined}");
    }
    assert!(iyon_tui::testing::style_at_text(&view, 80, &theme, "Migration").bold);
    assert!(iyon_tui::testing::style_at_text(&view, 80, &theme, "deprecated").strikethrough);
    assert!(iyon_tui::testing::style_at_text(&view, 80, &theme, "File").bold);
    assert!(joined.contains("[x] parsing"));
    assert!(!joined.contains("[x] - parsing"));
    assert_eq!(
        iyon_tui::testing::style_at_text(&view, 80, &theme, "rust").fg_rgb,
        Some((113, 128, 150))
    );
    assert!(iyon_tui::testing::style_at_text(&view, 80, &theme, "rust").dim);
    assert_eq!(
        iyon_tui::testing::style_at_text(&view, 80, &theme, "todo").fg_rgb,
        Some((120, 200, 210))
    );
}

#[test]
fn chunked_gfm_product_fixture_matches_whole() {
    let chunks = [
        "# Migr",
        "ation result\n\n",
        "The old path is ~~depre",
        "cated~~.\n\n| File |",
        " Status | Notes |\n| --- | :---:",
        " | --- |\n| api.rs | done | shared renderer |\n| ui.rs | pending | needs review |\n\n",
        "- [x] parsing\n- [x] rendering\n- [ ] cleanup\n\n",
        "> The old configuration remains compatible.\n\n",
        "```rust\nfn migrate() {\n    todo!();\n}\n```\n\n---\n",
    ];
    assert_eq!(chunks.concat(), MIGRATION_FIXTURE);

    let mut chunked = AssistantStream::new();
    for chunk in chunks {
        chunked.push_delta_paced(SegmentKind::Text, chunk);
        advance_some(&mut chunked, 4);
    }
    chunked.seal();
    let whole = sealed_text(MIGRATION_FIXTURE);
    assert_eq!(chunked.semantic, whole.semantic);
    for width in [8, 20, 40, 80] {
        assert_eq!(
            painted(&chunked.snapshot().view_for_test(), width),
            painted(&whole.snapshot().view_for_test(), width),
            "chunked vs whole physical mismatch at width {width}"
        );
    }
}

#[test]
fn gfm_fixture_source_provenance_stays_exact() {
    let stream = sealed_text(MIGRATION_FIXTURE);
    let mut runs = Runs::default();
    for span in stream.semantic.spans() {
        for value in span.values() {
            walk_content(&mut runs, value);
        }
    }
    for (provenance, _) in &runs.annotated_provenances {
        match provenance {
            TextProvenance::Exact(range) => {
                let slice = exact_source_slice(MIGRATION_FIXTURE, provenance)
                    .expect("exact provenance stays inside source");
                assert!(
                    range.end().as_u64() <= MIGRATION_FIXTURE.len() as u64,
                    "exact range escapes source: {range:?}"
                );
                assert!(
                    MIGRATION_FIXTURE.contains(slice),
                    "exact run is not in source: {slice:?}"
                );
            }
            TextProvenance::Derived(_) | TextProvenance::Synthetic => {}
        }
    }
    assert!(has_strike(&stream, "deprecated"));
}

#[test]
fn commonmark_corpus_still_projects_through_gfm_assistant() {
    let source = concat!(
        "# ATX\n\n",
        "Setext\n======\n\n",
        "[link][ref]\n\n",
        "[ref]: /url\n\n",
        "**hello _world_**\n\n",
        "inline `code`\n\n",
        "```\nfenced\n```\n\n",
        "> quote\n\n",
        "1. one\n2. two\n\n",
        "ordinary paragraph\n",
    );
    let stream = sealed_text(source);
    let kinds: Vec<_> = blocks(&stream)
        .iter()
        .map(|block| match block.kind() {
            BlockKind::Heading { level, .. } => format!("heading{}", level.get()),
            BlockKind::Paragraph(_) => "paragraph".into(),
            BlockKind::List(_) => "list".into(),
            BlockKind::CodeBlock(_) => "code".into(),
            BlockKind::BlockQuote { .. } => "quote".into(),
            BlockKind::ThematicBreak => "rule".into(),
            BlockKind::Table(_) => "table".into(),
            BlockKind::RawBlock { .. } => "raw".into(),
            BlockKind::Container { .. } => "container".into(),
            _ => "other".into(),
        })
        .collect();
    assert!(kinds.iter().any(|kind| kind == "heading1"));
    assert!(kinds.iter().filter(|kind| *kind == "heading1").count() >= 2);
    assert!(kinds.contains(&"list".to_string()));
    assert!(kinds.contains(&"code".to_string()));
    assert!(kinds.contains(&"quote".to_string()));
    assert!(kinds.contains(&"paragraph".to_string()));
    assert!(concat_marked(&stream, |marks, _| marks.contains(&Mark::Strong)).contains("hello"));
    assert!(concat_marked(&stream, |marks, _| marks.contains(&Mark::Code)).contains("code"));
    assert!(
        collect_runs(&stream)
            .marked
            .iter()
            .any(|(_, marks, _)| marks.iter().any(|mark| matches!(mark, Mark::Link(_))))
    );
}

#[test]
fn gfm_reference_compaction_still_respects_restart_floor() {
    let source = "[foo]: /target\n\nearlier paragraph\n\nlater [foo]";
    let mut stream = stream_with(&[(SegmentKind::Text, source)]);
    stream.seal();
    let before = stream.semantic.clone();
    let later = StreamOffset::new(source.find("later").expect("later block") as u64);
    StreamingSource::compact_before(&mut stream, later);
    let base = stream.snapshot().source_base();
    assert!(base < later, "reference context must pull restart backward");
    let expected: Vec<_> = before
        .spans()
        .iter()
        .filter(|span| span.source().end() > base)
        .map(|span| (span.source(), span.values().to_vec()))
        .collect();
    let actual: Vec<_> = stream
        .semantic
        .spans()
        .iter()
        .map(|span| (span.source(), span.values().to_vec()))
        .collect();
    assert_eq!(actual, expected);
}

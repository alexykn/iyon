use super::*;
use crate::transcript::pipeline::assistant_presentation;
use crate::tui::theme::iyon_theme;
use iyon_tui::text::{BlockKind, Mark, TextContent, TextOrigin, TextProvenance, walk_content};
use iyon_tui::{Block, History, View};

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
fn incomplete_markdown_guide_does_not_panic_the_pipeline() {
    let source = concat!(
        "What is Markdown?\n\n---\n\n",
        "```markdown\n# H1 Heading\n## H2 Heading\n",
        "Emphasis\n\n- Bold → **bold**\n- Italic → *italic*\n-",
        "\n\n> quote\n>\n> ---\n",
    );
    let mut stream = AssistantStream::new();
    for character in source.chars() {
        stream.push_delta_paced(SegmentKind::Text, &character.to_string());
        advance_some(&mut stream, 2);
    }
    stream.seal();
    assert!(
        !blocks(&stream).is_empty(),
        "sealed guide must still project blocks"
    );
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

    let lists: Vec<_> = lists_of(&stream);
    assert_eq!(
        lists.len(),
        2,
        "each completed task item is its own stable list"
    );
    assert_eq!(lists[0].items()[0].checked(), Some(true));
    assert_eq!(lists[1].items()[0].checked(), Some(false));
    assert_eq!(lists[0].items()[0].origin(), Some(TextOrigin::MARKDOWN));
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
fn fitting_tables_keep_content_tracks_aligned() {
    let stream = sealed_text(MIGRATION_FIXTURE);
    let view = assistant_view(&stream.semantic, &assistant_renderer());
    let lines = painted(&view, 80);

    assert_eq!(find_x(&lines, "File"), find_x(&lines, "api.rs"));
    assert_eq!(find_x(&lines, "File"), find_x(&lines, "ui.rs"));

    let status_x = find_x(&lines, "Status") as i32;
    let done_x = find_x(&lines, "done") as i32;
    let pending_x = find_x(&lines, "pending") as i32;
    assert!(
        (status_x - done_x).abs() <= 1 && (pending_x - done_x).abs() <= 1,
        "center column must sit in a content track, not a stretched Flex band: Status={status_x} done={done_x} pending={pending_x}"
    );

    let header = lines
        .iter()
        .find(|line| line.contains("File"))
        .expect("table header");
    assert!(
        header.trim_end().len() < 50,
        "fitting table must not stretch to the viewport: {header:?}"
    );
}

#[test]
fn delimiterless_pipe_rows_align_when_they_fit() {
    let languages = concat!(
        "| Python     | Dynamic | Very high | Data, scripting |\n",
        "| Dart       | Static  | Medium    | Flutter apps |\n",
        "| TypeScript| Static  | High      | Web apps |\n",
        "| Ruby       | Dynamic | Medium    | Web (Rails) |\n",
    );
    let stream = sealed_text(languages);
    assert_eq!(table_of(&stream).rows().len(), 4);
    let view = assistant_view(&stream.semantic, &assistant_renderer());
    let lines = painted(&view, 80);
    let joined = lines.join("\n");
    assert!(
        !joined.contains('|'),
        "pipe rows must become a Grid, not source markdown: {joined}"
    );
    assert_eq!(find_x(&lines, "Python"), find_x(&lines, "Dart"));
    assert_eq!(find_x(&lines, "Python"), find_x(&lines, "TypeScript"));
    assert_eq!(find_x(&lines, "Python"), find_x(&lines, "Ruby"));
    assert_eq!(find_x(&lines, "Dynamic"), find_x(&lines, "Static"));
    assert_eq!(find_x(&lines, "Very high"), find_x(&lines, "Medium"));

    let tasks = concat!(
        "| Fix bug #123 | High | ⌛ In progress |\n",
        "| Design logo | Medium | ⭕ Pending |\n",
        "| Update docs | Low | ⭕ Pending |\n",
    );
    let stream = sealed_text(tasks);
    let view = assistant_view(&stream.semantic, &assistant_renderer());
    let lines = painted(&view, 80);
    assert_eq!(find_x(&lines, "Fix bug"), find_x(&lines, "Design logo"));
    assert_eq!(find_x(&lines, "Fix bug"), find_x(&lines, "Update docs"));
    assert_eq!(find_x(&lines, "High"), find_x(&lines, "Medium"));
    assert_eq!(find_x(&lines, "High"), find_x(&lines, "Low"));
    assert_eq!(find_x(&lines, "In progress"), find_x(&lines, "Pending"));
}

const EMOJI_TABLE_FIXTURE: &str = r#"```markdown
| 🌿 Plant       | ☀️ Light     | 💧 Water    | 🌡️ Temp   | ⚠️ Warning      |
|---------------|-------------|-------------|-----------|-----------------|
| 🪴 Snake Plant | Low ☁️      | Every 3w   | 15-30°C   | ✅ Pet-safe     |
| 🌵 Cactus      | Direct ☀️   | Every 4w   | 20-35°C   | ⚠️ Spiky!      |
| 🌸 Orchid      | Bright 📎   | Weekly     | 18-28°C   | 🥶 No drafts    |
| 🌿 Monstera    | Medium 🌤️    | Bi-weekly  ||18-30°C   | ☣️ Toxic 🐱     |
| 🍃 Pothos      | Low-Med 🌥️  | When dry   -| 15-30°C   | ☣️ Toxic 🐾    | |
```

| 🌿 Plant       | ☀️ Light     | 💧 Water    | 🌡️ Temp   | ⚠️ Warning      |
|---------------|-------------|-------------|-----------|-----------------|
| 🪴 Snake Plant | Low ☁️      | Every 3w   | 15-30°C   | ✅ Pet-safe     |
| 🌵 Cactus      | Direct ☀️   | Every 4w   | 20-35°C   | ⚠️ Spiky!      |
| 🌸 Orchid      | Bright 📎   | Weekly     | 18-28°C   | 🥶 No drafts    |
| 🌿 Monstera    | Medium 🌤️    | Bi-weekly  ||18-30°C   | ☣️ Toxic 🐱     |
| 🍃 Pothos      | Low-Med 🌥️  | When dry   -| 15-30°C   | ☣️ Toxic 🐾    | |

---

🎉 Enjoy your tables! Each one is shown fenced (code block with markdown header)
and then unfenced (live rendered version) right below it. 🚀
"#;

#[test]
fn emoji_table_does_not_corrupt_following_prose() {
    let stream = sealed_text(EMOJI_TABLE_FIXTURE);
    let view = assistant_view(&stream.semantic, &assistant_renderer());
    let lines = painted(&view, 80);
    let joined = lines.join("\n");

    assert!(
        joined.contains("```") || lines.iter().any(|line| line.contains('|')),
        "fenced copy must keep raw pipes: {joined}"
    );
    assert!(
        joined.contains("fenced") && joined.contains("unfenced"),
        "closing sentence must stay intact: {joined}"
    );
    assert!(
        !joined.contains("fencedT"),
        "following prose must not pick up spilled table cells: {joined}"
    );

    let rule = lines
        .iter()
        .find(|line| line.contains('─'))
        .expect("thematic break");
    let spilled = rule
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>();
    assert!(
        spilled.is_empty(),
        "table cells must not leak onto the rule: {rule:?}"
    );
}

const RETRO_GAMING_TABLE: &str = r#"```markdown
| 🕹️ Game         | 🏷️ Genre   | 📅 Release | 👾 High Score  |
|:----------------|:------------|:----------:|:--------------:|
| Super Mario 🍄  | Platformer  | 1985       | 999,999 🏆     |
| Tetris 🧱       | Puzzle      | 1984       | 9,999,999 💎   |
| Sonic ⚡        | Platformer  | 1991       | 500,000 🥇     |
| Pac-Man 👻      | Arcade      | 1980       | 3,333,360 🎯   |
| Zelda 🗡️         | Adventure   | 1986       | N/A 🔑         |
```

| 🕹️ Game         | 🏷️ Genre   | 📅 Release | 👾 High Score  |
|:----------------|:------------|:----------:|:--------------:|
| Super Mario 🍄  | Platformer  | 1985       | 999,999 🏆     |
| Tetris 🧱       | Puzzle      | 1984       | 9,999,999 💎   |
| Sonic ⚡        | Platformer  | 1991       | 500,000 🥇     |
| Pac-Man 👻      | Arcade      | 1980       | 3,333,360 🎯   |
| Zelda 🗡️         | Adventure   | 1986       | N/A 🔑         |
"#;

#[test]
fn emoji_grid_does_not_spill_letters_into_neighbors() {
    let stream = sealed_text(RETRO_GAMING_TABLE);
    let view = assistant_view(&stream.semantic, &assistant_renderer());
    let lines = painted(&view, 80);
    let joined = lines.join("\n");
    assert!(
        !joined.contains("Game  e") && !joined.contains("Score   h") && !joined.contains("🗡️ n"),
        "wide glyphs must not wrap into neighboring cells: {joined}"
    );
    let unfenced: Vec<_> = lines
        .iter()
        .filter(|line| !line.contains('|'))
        .cloned()
        .collect();
    assert!(
        unfenced.iter().any(|line| line.contains("Super Mario")),
        "{joined}"
    );
    let mario_x = find_x(&unfenced, "Super Mario");
    let zelda_x = find_x(&unfenced, "Zelda");
    assert_eq!(mario_x, zelda_x, "{joined}");
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
    history
        .update_stream(handle, |source| {
            let joined = painted(&source.snapshot().view_for_test(), 40).join("\n");
            assert!(
                joined.contains('|'),
                "live GFM table stays unaligned pipes until seal: {joined}"
            );
        })
        .unwrap();
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

/// Numbered markdown guide matching the live-stream drop (1. then 7.).
const NUMBERED_GUIDE: &str = r#"Markdown: A Complete Guide

What is Markdown?

Markdown is a lightweight markup language created by John Gruber.

1. Headings

Use # followed by a space, up to six levels:

# Heading 1

## Heading 2

### Heading 3

2. Emphasis (Bold & Italic)

italic text — wrapped in single asterisks
**bold text** — wrapped in double asterisks

3. Lists

Unordered lists use -, *, or +:

- First item
- Second item
  - Nested item
- Third item

Ordered lists use numbers followed by a period:

1. First step
2. Second step
3. Third step

4. Links and Images

Links: [OpenAI](https://openai.com)

5. Code

Inline code uses single backticks: `print("hello")`

6. Blockquotes

> This is a blockquote.
> It can span multiple lines.

7. Horizontal Rules

Three or more hyphens:

---

8. Tables
"#;

const NUMBERED_GUIDE_TITLES: &[&str] = &[
    "1. Headings",
    "2. Emphasis",
    "3. Lists",
    "4. Links and Images",
    "5. Code",
    "6. Blockquotes",
    "7. Horizontal Rules",
    "8. Tables",
];

fn assert_titles_in_order(lines: &[String], titles: &[&str]) {
    let mut last = 0usize;
    for title in titles {
        let Some(index) = lines.iter().position(|line| line.contains(title)) else {
            panic!("missing {title:?} in:\n{}", lines.join("\n"));
        };
        assert!(
            index >= last,
            "{title:?} appeared at {index} before previous title at {last}:\n{}",
            lines.join("\n")
        );
        last = index;
    }
}

fn remember_titles(joined: &str, published: &mut Vec<&'static str>) {
    for title in NUMBERED_GUIDE_TITLES {
        if joined.contains(title) && !published.contains(title) {
            published.push(title);
        }
    }
}

fn assert_published_still_present(joined: &str, published: &[&str]) {
    for title in published {
        assert!(
            joined.contains(title),
            "lost {title:?} after it had already been painted\n{joined}"
        );
    }
}

#[test]
fn numbered_guide_keeps_every_section_in_the_painted_stream() {
    let stream = sealed_text(NUMBERED_GUIDE);
    let view = assistant_view(&stream.semantic, &assistant_renderer());
    let lines = painted(&view, 80);
    assert_titles_in_order(&lines, NUMBERED_GUIDE_TITLES);
}

#[test]
fn numbered_guide_keeps_every_section_while_live_prefixes_arrive() {
    let mut stream = AssistantStream::new();
    let mut published = Vec::new();
    for character in NUMBERED_GUIDE.chars() {
        stream.push_delta_paced(SegmentKind::Text, &character.to_string());
        advance_some(&mut stream, 2);
        let view = assistant_view(&stream.semantic, &assistant_renderer());
        let joined = painted(&view, 80).join("\n");
        remember_titles(&joined, &mut published);
        assert_published_still_present(&joined, &published);
    }
    drain_pacing(&mut stream);
    stream.seal();
    let view = assistant_view(&stream.semantic, &assistant_renderer());
    let lines = painted(&view, 80);
    assert_titles_in_order(&lines, NUMBERED_GUIDE_TITLES);
}

#[test]
fn numbered_guide_history_accepts_heading_after_numbered_item() {
    let mut history = History::new();
    let handle = history.push_stream(AssistantStream::new()).unwrap();
    for paragraph in NUMBERED_GUIDE.split_inclusive("\n\n") {
        history
            .update_stream(handle, |source| {
                source.push_delta_paced(SegmentKind::Text, paragraph);
                drain_pacing(source);
            })
            .expect("heading after a numbered item must not break History compaction");
    }
    history
        .seal_stream(handle)
        .expect("numbered guide must seal after compaction");
}

const NESTED_LISTS: &str = "\
- Fruits
  - Citrus
    - Oranges
    - Lemons
  - Berries
    - Strawberries
    - Blueberries
- Vegetables
  - Leafy greens
    - Spinach
    - Kale
  - Root vegetables
    - Carrots
    - Beets

1. Setup the environment
   1. Install dependencies
   2. Configure the settings file
2. Run the build
   1. Compile the source
   2. Run the test suite
      1. Unit tests
      2. Integration tests
";

const LONG_PARAGRAPH_LISTS: &str = "\
- The Quick Brown Fox
  The quick brown fox jumps over the lazy dog. This sentence is famous because it contains every letter of the English alphabet at least once, making it useful for testing fonts, keyboards, and displays. Despite its simplicity, it has appeared in countless typography manuals since the late 19th century.

- Lorem Ipsum Origins
  Lorem ipsum dolor sit amet, consectetur adipiscing elit. It is a long-established fact that a reader will be distracted by the readable content of a page when looking at its layout. The point of using placeholder text is that it has a more-or-less normal distribution of letters.
";

const LOOSE_LIST_THEN_MORE_ITEMS: &str = "\
- First item with a blank line inside

  This second paragraph makes the whole list loose.

- Short
- Also short
";

const NESTED_LONG_PARAGRAPHS_AND_TABLE: &str = "\
# Why Markdown Seems Not to Die

- Fruits
  - Citrus
    - Oranges
    - Lemons
  - Berries
    - Strawberries
- Vegetables
  - Leafy greens
    - Spinach

1. Setup the environment
   1. Install dependencies
   2. Configure the settings file
2. Run the build
   1. Compile the source
   2. Run the test suite
      1. Unit tests
      2. Integration tests

- The Quick Brown Fox
  The quick brown fox jumps over the lazy dog. This sentence is famous because it contains every letter of the English alphabet at least once, making it useful for testing fonts, keyboards, and displays.

- Lorem Ipsum Origins
  Lorem ipsum dolor sit amet, consectetur adipiscing elit. It is a long-established fact that a reader will be distracted by the readable content of a page when looking at its layout.

| Format | Born | Status |
| --- | --- | --- |
| Markdown | 2004 | Thriving |
| HTML | 1993 | Alive |

> Did you mean something else by \"it seems not to die\"?
";

const MARKDOWN_ESSAY_THEN_LISTS: &str = "\
# Why Markdown Seems Not to Die

You're right — Markdown has shown remarkable longevity. Created in 2004, it's now two decades old, yet it's more ubiquitous than ever.

1. It Solves a Timeless Problem

The need to write structured, portable, human-readable text is permanent. Word processors lock you into binary formats (.docx); Markdown stays as plain .txt with a few symbols.

2. No Vendor Lock-In

Proprietary formats die when companies do. Markdown is:

- An open convention, not a product
- Renderable by hundreds of independent tools
- Recoverable even if every Markdown app disappeared tomorrow

3. Network Effects & Critical Mass

Once GitHub, Reddit, Stack Overflow, and WhatsApp adopted it, it became a lingua franca.

4. It Evolves Without Breaking

Unlike brittle standards, Markdown survives via forks with backward compatibility:

- CommonMark formalizes the base
- GFM (GitHub Flavored) adds tables, task lists, etc.
- Pandoc's Markdown extends further

5. The Zombie Strength of Simplicity

Complex tools die when complexity outweighs benefit.

---

## Lists in Markdown

- Fruits
  - Citrus
    - Oranges
    - Lemons
  - Berries
    - Strawberries
- Vegetables
  - Leafy greens
    - Spinach
    - Kale

1. Setup the environment
   1. Install dependencies
   2. Configure the settings file
2. Run the build
   1. Compile the source
   2. Run the test suite
      1. Unit tests
      2. Integration tests

- The Quick Brown Fox
  The quick brown fox jumps over the lazy dog. This sentence is famous because it contains every letter of the English alphabet at least once, making it useful for testing fonts, keyboards, and displays.

- Lorem Ipsum Origins
  Lorem ipsum dolor sit amet, consectetur adipiscing elit. It is a long-established fact that a reader will be distracted by the readable content of a page when looking at its layout.

| Format | Born | Status |
| --- | --- | --- |
| LaTeX | 1984 | Alive |
| HTML | 1993 | Alive |
| Markdown | 2004 | Thriving |

> Did you mean something else by \"it seems not to die\"?
> For example, were you referring to a background process, a script, or a specific Markdown renderer that kept running?
";

fn history_accepts_chunks(label: &str, source: &str, chunks: impl IntoIterator<Item = String>) {
    let mut history = History::new();
    let handle = history.push_stream(AssistantStream::new()).unwrap();
    let mut consumed = 0usize;
    let mut assembled = String::new();
    for chunk in chunks {
        consumed += chunk.len();
        assembled.push_str(&chunk);
        history
            .update_stream(handle, |stream| {
                stream.push_delta_paced(SegmentKind::Text, &chunk);
                drain_pacing(stream);
            })
            .unwrap_or_else(|error| {
                panic!(
                    "{label} History compaction failed after {consumed} bytes ({chunk:?}): {error}"
                )
            });
    }
    assert_eq!(
        assembled, source,
        "{label}: test chunks must reconstruct source"
    );
    history
        .seal_stream(handle)
        .unwrap_or_else(|error| panic!("{label} History seal failed: {error}"));
}

fn history_accepts_live_pacing(label: &str, source: &str) {
    let mut history = History::new();
    let handle = history.push_stream(AssistantStream::new()).unwrap();
    history
        .update_stream(handle, |stream| {
            stream.push_delta_paced(SegmentKind::Text, source);
        })
        .unwrap_or_else(|error| panic!("{label} initial History refresh failed: {error}"));
    let mut now = Instant::now();
    for step in 0..200_000 {
        now += Duration::from_millis(16);
        let (progressed, wakeup) = history
            .update_stream(handle, |stream| {
                let progressed = stream.advance(now);
                (progressed, stream.next_wakeup())
            })
            .unwrap_or_else(|error| panic!("{label} live pacing step {step}: {error}"));
        if !progressed && wakeup.is_none() {
            break;
        }
    }
    history
        .seal_stream(handle)
        .unwrap_or_else(|error| panic!("{label} History seal after live pacing failed: {error}"));
}

fn paragraph_chunks(source: &str) -> Vec<String> {
    source.split_inclusive("\n\n").map(str::to_owned).collect()
}

fn line_chunks(source: &str) -> Vec<String> {
    source.split_inclusive('\n').map(str::to_owned).collect()
}

fn char_chunks(source: &str) -> Vec<String> {
    source.chars().map(|ch| ch.to_string()).collect()
}

fn debug_first_diff(before: &str, after: &str) -> String {
    for (line, (left, right)) in before.lines().zip(after.lines()).enumerate() {
        if left != right {
            return format!("first ir diff at line {line}:\n  before: {left}\n  after:  {right}");
        }
    }
    format!(
        "ir length before={} after={}",
        before.lines().count(),
        after.lines().count()
    )
}

fn compact_stable_prefix_must_keep_suffix_views(label: &str, source: &str) {
    let mut probe = stream_with(&[(SegmentKind::Text, source)]);
    drain_pacing(&mut probe);
    let renderer = assistant_renderer();
    let chunks = assistant_presentation(&probe.semantic, &renderer);
    if chunks.len() < 2 {
        return;
    }
    let offsets: Vec<_> = chunks
        .iter()
        .take(chunks.len() - 1)
        .map(|chunk| chunk.source.end())
        .filter(|end| {
            *end <= probe.semantic.stable_through() && *end > probe.semantic.source_base()
        })
        .collect();
    for offset in offsets {
        let mut stream = stream_with(&[(SegmentKind::Text, source)]);
        drain_pacing(&mut stream);
        if stream.pipeline.restart_from(offset) != offset {
            continue;
        }
        let before = assistant_presentation(&stream.semantic, &renderer);
        let before_suffix: Vec<_> = before
            .iter()
            .filter(|chunk| chunk.source.start() >= offset)
            .map(|chunk| chunk.view.clone())
            .collect();
        StreamingSource::compact_before(&mut stream, offset);
        let after: Vec<_> = assistant_presentation(&stream.semantic, &renderer)
            .into_iter()
            .map(|chunk| chunk.view)
            .collect();
        if after != before_suffix {
            let before_dbg = format!("{before_suffix:#?}");
            let after_dbg = format!("{after:#?}");
            let diff = debug_first_diff(&before_dbg, &after_dbg);
            panic!(
                "{label}: compact at {offset:?} changed remaining presentation\n\
                 {diff}\n\
                 before suffix:\n{}\n\
                 after:\n{}",
                painted_joined(
                    &View::vertical(|column| {
                        column.children(before_suffix.iter().cloned());
                    }),
                    80
                ),
                painted_joined(
                    &View::vertical(|column| {
                        column.children(after.iter().cloned());
                    }),
                    80
                ),
            );
        }
    }
}

fn token_chunks(source: &str, chars_per_chunk: usize) -> Vec<String> {
    let chars: Vec<char> = source.chars().collect();
    chars
        .chunks(chars_per_chunk.max(1))
        .map(|chunk| chunk.iter().collect())
        .collect()
}

fn history_accepts_token_chunks_with_live_pacing(
    label: &str,
    source: &str,
    chars_per_chunk: usize,
) {
    let mut history = History::new();
    let handle = history.push_stream(AssistantStream::new()).unwrap();
    let mut now = Instant::now();
    let mut consumed = 0usize;
    for chunk in token_chunks(source, chars_per_chunk) {
        consumed += chunk.len();
        now += Duration::from_millis(16);
        history
            .update_stream(handle, |stream| {
                stream.push_delta_paced(SegmentKind::Text, &chunk);
                stream.advance(now);
            })
            .unwrap_or_else(|error| {
                panic!("{label} token chunk after {consumed} bytes ({chunk:?}): {error}")
            });
    }
    for step in 0..200_000 {
        now += Duration::from_millis(16);
        let (progressed, wakeup) = history
            .update_stream(handle, |stream| {
                let progressed = stream.advance(now);
                (progressed, stream.next_wakeup())
            })
            .unwrap_or_else(|error| panic!("{label} drain step {step}: {error}"));
        if !progressed && wakeup.is_none() {
            break;
        }
    }
    history
        .seal_stream(handle)
        .unwrap_or_else(|error| panic!("{label} seal failed: {error}"));
}

#[test]
fn history_accepts_nested_lists_paragraph_chunks() {
    history_accepts_chunks("nested lists", NESTED_LISTS, paragraph_chunks(NESTED_LISTS));
}

#[test]
fn history_accepts_nested_lists_line_chunks() {
    history_accepts_chunks(
        "nested lists lines",
        NESTED_LISTS,
        line_chunks(NESTED_LISTS),
    );
}

#[test]
fn history_accepts_long_paragraph_lists_line_chunks() {
    history_accepts_chunks(
        "long paragraph lists",
        LONG_PARAGRAPH_LISTS,
        line_chunks(LONG_PARAGRAPH_LISTS),
    );
}

#[test]
fn history_accepts_loose_list_line_chunks() {
    history_accepts_chunks(
        "loose list",
        LOOSE_LIST_THEN_MORE_ITEMS,
        line_chunks(LOOSE_LIST_THEN_MORE_ITEMS),
    );
}

#[test]
fn history_accepts_nested_long_paragraphs_and_table_line_chunks() {
    history_accepts_chunks(
        "nested+paragraphs+table",
        NESTED_LONG_PARAGRAPHS_AND_TABLE,
        line_chunks(NESTED_LONG_PARAGRAPHS_AND_TABLE),
    );
}

#[test]
fn history_accepts_nested_long_paragraphs_and_table_char_chunks() {
    history_accepts_chunks(
        "nested+paragraphs+table chars",
        NESTED_LONG_PARAGRAPHS_AND_TABLE,
        char_chunks(NESTED_LONG_PARAGRAPHS_AND_TABLE),
    );
}

#[test]
fn compacting_nested_lists_keeps_suffix_views() {
    compact_stable_prefix_must_keep_suffix_views("nested lists", NESTED_LISTS);
}

#[test]
fn compacting_long_paragraph_lists_keeps_suffix_views() {
    compact_stable_prefix_must_keep_suffix_views("long paragraph lists", LONG_PARAGRAPH_LISTS);
}

#[test]
fn compacting_loose_list_keeps_suffix_views() {
    compact_stable_prefix_must_keep_suffix_views("loose list", LOOSE_LIST_THEN_MORE_ITEMS);
}

#[test]
fn compacting_nested_long_paragraphs_and_table_keeps_suffix_views() {
    compact_stable_prefix_must_keep_suffix_views(
        "nested+paragraphs+table",
        NESTED_LONG_PARAGRAPHS_AND_TABLE,
    );
}

#[test]
fn history_accepts_nested_long_paragraphs_and_table_live_pacing() {
    history_accepts_live_pacing(
        "nested+paragraphs+table live",
        NESTED_LONG_PARAGRAPHS_AND_TABLE,
    );
}

#[test]
fn history_accepts_nested_long_paragraphs_and_table_token_chunks() {
    history_accepts_token_chunks_with_live_pacing(
        "nested+paragraphs+table tokens",
        NESTED_LONG_PARAGRAPHS_AND_TABLE,
        7,
    );
}

#[test]
fn history_accepts_markdown_essay_then_lists_live_pacing() {
    history_accepts_live_pacing("essay then lists live", MARKDOWN_ESSAY_THEN_LISTS);
}

#[test]
fn history_accepts_markdown_essay_then_lists_line_chunks() {
    history_accepts_chunks(
        "essay then lists lines",
        MARKDOWN_ESSAY_THEN_LISTS,
        line_chunks(MARKDOWN_ESSAY_THEN_LISTS),
    );
}

#[test]
fn history_accepts_markdown_essay_then_lists_char_chunks() {
    history_accepts_chunks(
        "essay then lists chars",
        MARKDOWN_ESSAY_THEN_LISTS,
        char_chunks(MARKDOWN_ESSAY_THEN_LISTS),
    );
}

#[test]
fn history_accepts_markdown_essay_then_lists_token_chunks() {
    history_accepts_token_chunks_with_live_pacing(
        "essay then lists tokens",
        MARKDOWN_ESSAY_THEN_LISTS,
        5,
    );
}

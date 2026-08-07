//! Semantic-block projection of the active turn.
//!
//! The commit cursor is width-**independent**: it tracks how far into the source
//! stream (by character) has been committed to native history, not how many
//! physical rows were produced at some width. On resize the committed blocks
//! are untouched and only the open tail is rewrapped.
//!
//! Blocks are conservative: anything that could still change stays open
//! (Stable = false) and lives in the live tail. Finalized/`Final` mode marks
//! every remaining block stable.

use ratatui::text::Line;

use crate::{
    runtime::state::{ActiveTurn, TurnId},
    transcript::{AssistantSegment, SegmentKind, model::TuiFormatter, wrap::wrap_transcript_rows},
};

/// Identity of a semantic block within a turn (turn id + sequence).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct BlockId {
    pub(crate) turn: TurnId,
    pub(crate) sequence: u32,
}

/// One semantic block of the active turn: a width-independent source boundary
/// plus its physical rows at the current width.
#[derive(Debug, Clone)]
pub(crate) struct ProjectedBlock {
    pub(crate) id: BlockId,
    /// Index in the concatenated (kind-annotated) source at which this block's
    /// content ends. Committing blocks advances the cursor to this boundary.
    pub(crate) source_end: usize,
    /// Physical rows at the current width.
    pub(crate) rows: Vec<Line<'static>>,
    pub(crate) stable: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct TurnProjection {
    pub(crate) blocks: Vec<ProjectedBlock>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectionMode {
    Streaming,
    Final,
}

/// A single logical line of the source stream (kept kind-annotated so styling
/// survives). Also tracks absolute char offset.
#[derive(Debug, Clone)]
struct LogicalLine {
    kind: SegmentKind,
    text: String,
    /// Last char index of this line's text (end boundary).
    end_char: usize,
    /// Whether the line ends with a newline (terminated).
    terminated: bool,
}

/// Splits the source segments into logical lines with absolute char offsets in
/// the concatenated, kind-annotated text. A line is `terminated` iff it ended
/// with a newline in the source (i.e. its size is fixed and won't grow).
fn logical_lines(segments: &[AssistantSegment]) -> Vec<LogicalLine> {
    let mut lines: Vec<LogicalLine> = Vec::new();
    let mut char_cursor = 0usize;

    for segment in segments {
        let kind = match segment {
            AssistantSegment::Text(_) => SegmentKind::Text,
            AssistantSegment::Thinking(_) => SegmentKind::Thinking,
        };
        let text = segment.text();
        let mut start = 0usize;
        while start < text.len() {
            let cut = text[start..].find('\n');
            let (piece_end, terminated) = match cut {
                Some(rel) => (start + rel, true),
                None => (text.len(), false),
            };
            let piece = &text[start..piece_end];
            if !piece.is_empty() {
                char_cursor += piece.chars().count();
                lines.push(LogicalLine {
                    kind,
                    text: piece.to_string(),
                    end_char: char_cursor,
                    terminated,
                });
            }
            if terminated {
                char_cursor += 1; // the newline
                start = piece_end + 1;
            } else {
                break;
            }
        }
    }

    lines
}

/// Project the active turn into semantic blocks at `width`.
pub(crate) fn project_active_turn(
    turn: &ActiveTurn,
    width: u16,
    formatter: &TuiFormatter,
    mode: ProjectionMode,
) -> TurnProjection {
    let lines = logical_lines(&turn.segments);
    let mut blocks = Vec::with_capacity(lines.len());

    for (i, line) in lines.iter().enumerate() {
        let final_line = i == lines.len() - 1;
        // A block's source boundary is the char index after the line, including
        // the trailing newline when it is terminated. This is width-independent.
        let source_end = line.end_char + usize::from(line.terminated);

        let stable = match mode {
            ProjectionMode::Final => true,
            ProjectionMode::Streaming => {
                // A newline-terminated line is stable unless it's the final
                // (still-growing) line. Unclosed markdown markers on an
                // otherwise-terminated line are conservatively kept open via
                // the restricted metadata check below.
                line.terminated && !final_line
            }
        };
        let stable = stable && !is_restricted_line(formatter, line, mode);

        let block_rows = render_logical_line(formatter, width, line);
        blocks.push(ProjectedBlock {
            id: BlockId {
                turn: turn.id,
                sequence: i as u32,
            },
            source_end,
            rows: block_rows,
            stable,
        });
    }

    TurnProjection { blocks }
}

/// Renders a single logical line to physical rows (with markdown + margin
/// treatment) at the current width.
fn render_logical_line(
    formatter: &TuiFormatter,
    width: u16,
    line: &LogicalLine,
) -> Vec<Line<'static>> {
    let meta = formatter.format_assistant_body_meta(&[segments_for(line)]);
    let logical: Vec<_> = meta.into_iter().map(|r| r.row).collect();
    wrap_transcript_rows(width.max(1), &logical)
}

fn segments_for(line: &LogicalLine) -> AssistantSegment {
    match line.kind {
        SegmentKind::Text => AssistantSegment::Text(line.text.clone()),
        SegmentKind::Thinking => AssistantSegment::Thinking(line.text.clone()),
    }
}

/// Conservative: if formatting this logical line reports it `restricted` (hides
/// an unclosed markdown marker), keep it open even if newline-terminated.
fn is_restricted_line(
    formatter: &TuiFormatter,
    line: &LogicalLine,
    mode: ProjectionMode,
) -> bool {
    if mode == ProjectionMode::Final {
        return false;
    }
    let meta = formatter.format_assistant_body_meta(&[segments_for(line)]);
    meta.iter().any(|r| r.restricted)
}

/// The split of a projection into (newly-stable rows to commit, live tail rows),
/// given the already-committed source boundary.
pub(crate) struct StableAndTail {
    pub(crate) stable_rows: Vec<Line<'static>>,
    pub(crate) tail_rows: Vec<Line<'static>>,
    /// New committed source boundary (end of the last stable block selected).
    pub(crate) new_source_end: usize,
}

/// Selects stable blocks after `committed_source_end` as history rows and open
/// blocks as the live tail. Width-independent: the cursor is a source boundary.
pub(crate) fn split_projection(
    projection: &TurnProjection,
    committed_source_end: usize,
) -> StableAndTail {
    let mut stable_rows = Vec::new();
    let mut tail_rows = Vec::new();
    let mut new_source_end = committed_source_end;

    for block in &projection.blocks {
        if block.source_end <= committed_source_end {
            continue;
        }
        if block.stable {
            stable_rows.extend(block.rows.iter().cloned());
            new_source_end = block.source_end;
        } else {
            tail_rows.extend(block.rows.iter().cloned());
        }
    }

    StableAndTail {
        stable_rows,
        tail_rows,
        new_source_end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::AssistantSegment;

    fn turn(text: &str, id: TurnId) -> ActiveTurn {
        ActiveTurn {
            id,
            segments: vec![AssistantSegment::Text(text.to_string())],
            revision: 0,
            status: crate::runtime::state::ActiveTurnStatus::Streaming,
        }
    }

    #[test]
    fn stable_cursor_is_width_independent() {
        // Same source: at both widths the committed source_end must be identical
        // (a semantic boundary), even though physical row counts differ.
        let t = turn("line one\nline two", TurnId(1));
        let formatter = TuiFormatter::default();

        let wide = project_active_turn(&t, 100, &formatter, ProjectionMode::Streaming);
        let narrow = project_active_turn(&t, 10, &formatter, ProjectionMode::Streaming);

        let sw = split_projection(&wide, 0);
        let sn = split_projection(&narrow, 0);
        // First line is terminated+not-final => stable at both widths.
        assert_eq!(sw.new_source_end, sn.new_source_end);
        assert!(sw.new_source_end > 0);
    }

    #[test]
    fn unfinished_paragraph_remains_open() {
        let t = turn("growing text", TurnId(1));
        let formatter = TuiFormatter::default();
        let proj = project_active_turn(&t, 80, &formatter, ProjectionMode::Streaming);
        assert!(!proj.blocks[0].stable);
        let split = split_projection(&proj, 0);
        assert!(split.stable_rows.is_empty());
        assert!(!split.tail_rows.is_empty());
    }

    #[test]
    fn closed_paragraph_becomes_stable() {
        let t = turn("done line\nnext", TurnId(1));
        let formatter = TuiFormatter::default();
        let proj = project_active_turn(&t, 80, &formatter, ProjectionMode::Streaming);
        // Block 0 ("done line\n") is terminated and not final => stable.
        assert!(proj.blocks[0].stable);
        // Block 1 ("next") is the final growing line => open.
        assert!(!proj.blocks[1].stable);
    }

    #[test]
    fn final_mode_stabilizes_remaining_blocks() {
        let t = turn("one\ntwo\nthree", TurnId(1));
        let formatter = TuiFormatter::default();
        let proj = project_active_turn(&t, 80, &formatter, ProjectionMode::Final);
        assert!(proj.blocks.iter().all(|b| b.stable));
    }

    /// Property: commit progress (new_source_end) is identical regardless of
    /// terminal width, and committing across widths never decreases the cursor
    /// or skips/duplicates blocks.
    proptest::proptest! {
        #[test]
        fn commit_cursor_is_width_independent_across_resizes(
            lines in proptest::collection::vec("[a-z]+", 1..6),
        ) {
            let text = lines.join("\n");
            let t = turn(&text, TurnId(99));
            let formatter = TuiFormatter::default();

            let proj = project_active_turn(&t, 80, &formatter, ProjectionMode::Streaming);
            let split = split_projection(&proj, 0);
            let committed_after_first = split.new_source_end;

            // Simulate a resize: reproject at a different width from the same
            // committed boundary. The resulting cursor must not move backward.
            let resized = project_active_turn(&t, 7, &formatter, ProjectionMode::Streaming);
            let resplit = split_projection(&resized, committed_after_first);
            proptest::prop_assert!(resplit.new_source_end >= committed_after_first);

            // Every stable block is selected at most once: selecting from the
            // returned cursor yields no further stable rows from the same source.
            let again = split_projection(&resized, resplit.new_source_end);
            proptest::prop_assert!(again.stable_rows.is_empty());
        }
    }
}

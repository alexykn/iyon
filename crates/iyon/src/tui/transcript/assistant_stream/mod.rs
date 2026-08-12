//! Assistant-specific streaming adapter implementing [`StreamingSource`].
//!
//! Encapsulates Thinking styling, Markdown interpretation, thinking -> text
//! newline separation, and the width-independent stability frontier.

use unicode_width::UnicodeWidthStr;

use crate::transcript::markdown::{
    AssistantContinuation, AssistantDocument, AssistantRowLayout, parse_assistant,
    parse_assistant_tail,
};
use crate::transcript::semantic::{
    AssistantSegment, SegmentKind, slice_segments, think_to_text_newline,
};
use std::time::Instant;

use iyon_tui::{
    HorizontalAlign, ProjectedText, ProjectionBuilder, Projector, Smooth, StreamOffset,
    StreamRange, StreamRevision, StreamSnapshot, StreamSnapshotBuilder, StreamingSource, StyleRef,
    WrapMode,
};

#[derive(Debug)]
pub(crate) struct AssistantStream {
    /// Semantic segments already released by Smooth and visible to History.
    segments: Vec<AssistantSegment>,
    /// Full received semantic input, retained for normalization and projection.
    received_segments: Vec<AssistantSegment>,
    source_base: StreamOffset,
    source_end: StreamOffset,
    received_end: StreamOffset,
    pacing_atoms: Vec<(StreamRange, AssistantPacingAtom)>,
    smoother: Smooth,
    continuation: Option<AssistantContinuation>,
    revision: StreamRevision,
    sealed: bool,
}

#[derive(Debug, Clone)]
struct AssistantPacingAtom {
    kind: SegmentKind,
    text: String,
}

fn append_segment(segments: &mut Vec<AssistantSegment>, kind: SegmentKind, text: &str) {
    if text.is_empty() {
        return;
    }
    match kind {
        SegmentKind::Text => match segments.last_mut() {
            Some(AssistantSegment::Text(existing)) => existing.push_str(text),
            _ => segments.push(AssistantSegment::Text(text.to_owned())),
        },
        SegmentKind::Thinking => match segments.last_mut() {
            Some(AssistantSegment::Thinking(existing)) => existing.push_str(text),
            _ => segments.push(AssistantSegment::Thinking(text.to_owned())),
        },
    }
}

impl AssistantStream {
    pub(crate) fn new() -> Self {
        Self {
            segments: Vec::new(),
            received_segments: Vec::new(),
            source_base: StreamOffset::ZERO,
            source_end: StreamOffset::ZERO,
            received_end: StreamOffset::ZERO,
            pacing_atoms: Vec::new(),
            smoother: Smooth::default(),
            continuation: None,
            revision: StreamRevision::ZERO,
            sealed: false,
        }
    }

    /// Appends and exposes a complete delta. This remains the unsmoothed semantic
    /// helper used by the Markdown oracle tests; the application uses the paced
    /// variant below.
    pub(crate) fn push_delta(&mut self, kind: SegmentKind, chunk: &str) {
        self.receive_delta(kind, chunk);
        self.segments = self.received_segments.clone();
        self.source_end = self.received_end;
        self.smoother = Smooth::default();
        self.revision = self.revision.next();
    }

    pub(crate) fn push_delta_paced(&mut self, kind: SegmentKind, chunk: &str) {
        self.receive_delta(kind, chunk);
        self.refresh_smoothing();
    }

    fn receive_delta(&mut self, kind: SegmentKind, chunk: &str) {
        assert!(!self.sealed, "cannot append to sealed AssistantStream");
        if chunk.is_empty() {
            return;
        }

        let chunk = think_to_text_newline(&self.received_segments, kind, chunk).into_owned();
        append_segment(&mut self.received_segments, kind, &chunk);
        let mut cursor = self.received_end;
        for character in chunk.chars() {
            let end = cursor.saturating_add(character.len_utf8() as u64);
            self.pacing_atoms.push((
                StreamRange::new(cursor, end),
                AssistantPacingAtom {
                    kind,
                    text: character.to_string(),
                },
            ));
            cursor = end;
        }
        self.received_end = cursor;
    }

    fn refresh_smoothing(&mut self) {
        let mut builder = ProjectionBuilder::new(
            self.source_base,
            self.received_end,
            self.received_end,
            self.sealed,
        );
        for (range, atom) in self
            .pacing_atoms
            .iter()
            .filter(|(range, _)| range.end() > self.source_base)
        {
            builder = builder.emit_many(*range, [atom.clone()]);
        }
        let input = builder
            .finish()
            .expect("assistant pacing projection must be valid");
        let output = self.smoother.project(&input).expect("Smooth is infallible");
        let previous_end = self.source_end;
        for span in output.spans() {
            if span.source().end() <= previous_end {
                continue;
            }
            for atom in span.values() {
                append_segment(&mut self.segments, atom.kind, &atom.text);
            }
            self.source_end = span.source().end();
        }
        if self.source_end != previous_end {
            self.revision = self.revision.next();
        }
    }

    fn restart_plan_at(
        &self,
        offset: StreamOffset,
    ) -> (StreamOffset, Option<AssistantContinuation>) {
        let doc = parse_assistant(&self.received_segments);
        let target = offset.as_u64() as usize;
        let mut cursor = 0usize;

        for row in &doc.rows {
            let row_end = cursor + row.source.total_len();
            if target < row_end {
                let mut restart = target;
                for run in &row.projected_runs {
                    let run_start = cursor + run.owned.start;
                    let run_end = cursor + run.owned.end;
                    if target >= run_start && target < run_end {
                        // Source projection determines where a stream may be
                        // physically cut. Parser restart metadata determines
                        // how much earlier source is needed to reconstruct the
                        // semantic suffix. Exact projection does not imply
                        // parser independence.
                        let restart_from = match run.exact_visible.as_ref() {
                            Some(visible) => {
                                let visible_start = cursor + visible.start;
                                if target < visible_start {
                                    run.prefix_restart_from
                                } else {
                                    run.restart_from
                                }
                            }
                            None => run.restart_from.or(run.prefix_restart_from),
                        };
                        restart = restart_from.map_or(target, |restart| cursor + restart);
                        break;
                    }
                }
                let continuation = if restart > cursor {
                    Some(match row.layout {
                        AssistantRowLayout::Heading => AssistantContinuation::Heading,
                        AssistantRowLayout::Plain => AssistantContinuation::Paragraph,
                        AssistantRowLayout::ListItem { depth, marker } => {
                            AssistantContinuation::List {
                                body_column: list_body_column(depth, &marker),
                            }
                        }
                        AssistantRowLayout::ListContinuation { body_column } => {
                            AssistantContinuation::List { body_column }
                        }
                    })
                } else {
                    None
                };
                return (StreamOffset::new(restart as u64), continuation);
            }
            if target == row_end {
                return (offset, None);
            }
            cursor = row_end;
        }
        (offset, None)
    }
}

fn list_body_column(depth: usize, marker: &crate::transcript::markdown::AssistantMarker) -> u16 {
    let marker_width = match marker {
        crate::transcript::markdown::AssistantMarker::Bullet => 2,
        crate::transcript::markdown::AssistantMarker::Ordered { index } => {
            index.to_string().len() as u16 + 2
        }
    };
    (depth as u16)
        .saturating_mul(crate::transcript::markdown::ASSISTANT_LIST_INDENT as u16)
        .saturating_add(marker_width)
}

impl StreamingSource for AssistantStream {
    fn snapshot(&self) -> StreamSnapshot {
        let tail_segments = slice_segments(
            &self.segments,
            self.source_base.as_u64() as usize,
            self.source_end.as_u64() as usize,
        );
        let doc = parse_assistant_tail(&tail_segments, self.source_base, self.continuation);
        let (nodes, stable_through) =
            build_assistant_stream_view(&doc, self.source_base, self.source_end, self.sealed);
        let mut builder = StreamSnapshotBuilder::new(
            self.revision,
            self.source_base,
            stable_through,
            self.source_end,
        );
        for node in nodes {
            builder = builder.projected_text(node);
        }
        builder
            .finish()
            .expect("AssistantStream snapshot must be valid")
    }

    fn compact_before(&mut self, offset: StreamOffset) {
        if offset <= self.source_base {
            return;
        }

        debug_assert!(offset <= self.source_end);

        let (restart, continuation) = self.restart_plan_at(offset);
        self.continuation = continuation;
        self.source_base = restart;
        self.pacing_atoms.retain(|(range, _)| range.end() > restart);
        self.revision = self.revision.next();
    }

    fn seal(&mut self) {
        if self.sealed {
            return;
        }
        self.sealed = true;
        self.refresh_smoothing();
        self.revision = self.revision.next();
    }

    fn is_sealed(&self) -> bool {
        self.sealed
    }

    fn next_wakeup(&self) -> Option<Instant> {
        (!self.sealed)
            .then(|| self.smoother.next_wakeup())
            .flatten()
    }

    fn advance(&mut self, now: Instant) -> bool {
        if self.sealed || !self.smoother.advance(now) {
            return false;
        }
        let previous = self.source_end;
        self.refresh_smoothing();
        self.source_end > previous
    }
}

/// Builds projected semantic text nodes and computes the width-independent stability frontier.
fn build_assistant_stream_view(
    doc: &AssistantDocument,
    source_base: StreamOffset,
    source_end: StreamOffset,
    sealed: bool,
) -> (Vec<ProjectedText>, StreamOffset) {
    let mut nodes = Vec::new();
    let mut cursor = source_base;
    let mut stable_through = source_base;

    for row in &doc.rows {
        let text_end = cursor.saturating_add(row.source.content_len as u64);
        let text_range = StreamRange::new(cursor, text_end);
        let list_layout = match &row.layout {
            AssistantRowLayout::ListItem { depth, marker } => {
                let marker_text = match marker {
                    crate::transcript::markdown::AssistantMarker::Bullet => "• ".to_string(),
                    crate::transcript::markdown::AssistantMarker::Ordered { index } => {
                        format!("{index}. ")
                    }
                };
                Some((
                    row.projected_runs
                        .first()
                        .map_or(row.source.content_len, |run| run.owned.start),
                    list_body_column(*depth, marker),
                    format!(
                        "{}{}",
                        " ".repeat(*depth * crate::transcript::markdown::ASSISTANT_LIST_INDENT),
                        marker_text
                    ),
                    true,
                ))
            }
            AssistantRowLayout::ListContinuation { body_column } => {
                Some((0, *body_column, String::new(), false))
            }
            _ => None,
        };

        let (body_start, layout) = list_layout.map_or(
            (0, None),
            |(body_start, body_column, prefix, show_prefix)| {
                if show_prefix {
                    debug_assert_eq!(
                        UnicodeWidthStr::width(prefix.as_str()),
                        usize::from(body_column)
                    );
                }
                (body_start, Some((body_column, prefix, show_prefix)))
            },
        );
        let mut builder = ProjectedText::builder(text_range)
            .fill_width()
            .wrap(WrapMode::WordThenGrapheme)
            .align(HorizontalAlign::Start);
        if let Some((body_column, prefix, show_prefix)) = layout {
            builder = builder.hanging(
                body_column,
                prefix,
                StyleRef::from(row.style.clone()),
                StreamRange::new(cursor, cursor.saturating_add(body_start as u64)),
                show_prefix,
            );
        }
        for run in if row.projected_runs.is_empty() {
            let mut run_cursor = cursor;
            row.spans
                .iter()
                .map(|span| {
                    let start = run_cursor;
                    run_cursor = run_cursor.saturating_add(span.text().len() as u64);
                    (
                        span.text().to_owned(),
                        span.style().clone(),
                        StreamRange::new(start, run_cursor),
                        Some(StreamRange::new(start, run_cursor)),
                    )
                })
                .collect::<Vec<_>>()
        } else {
            row.projected_runs
                .iter()
                .map(|run| {
                    let owned = StreamRange::new(
                        cursor.saturating_add(run.owned.start as u64),
                        cursor.saturating_add(run.owned.end as u64),
                    );
                    let exact = run.exact_visible.as_ref().map(|visible| {
                        StreamRange::new(
                            cursor.saturating_add(visible.start as u64),
                            cursor.saturating_add(visible.end as u64),
                        )
                    });
                    (run.display.clone(), run.style.clone().into(), owned, exact)
                })
                .collect()
        } {
            builder = builder.run(run.0, run.2, run.3, run.1);
        }
        if row.source.has_newline {
            builder = builder.hard_newline();
        }
        let projected = builder
            .finish()
            .expect("AssistantStream must build valid projected text");
        let owned_end = projected.owned_range().end();
        nodes.push(projected);

        if sealed || row.source.has_newline {
            stable_through = owned_end;
        } else {
            stable_through = cursor.saturating_add(row.source.stable_prefix_len as u64);
        }
        cursor = owned_end;
    }

    if sealed || nodes.is_empty() {
        stable_through = source_end;
    }
    (nodes, stable_through.min(source_end))
}
#[cfg(test)]
mod tests;

//! Assistant-specific streaming adapter implementing [`StreamingSource`].
//!
//! Encapsulates Thinking styling, Markdown interpretation, thinking -> text
//! newline separation, and the width-independent stability frontier.

use unicode_width::UnicodeWidthStr;

use crate::presentation::{HorizontalAlign, WidthRule, WrapMode};
use crate::stream::{
    ExactTerminator, ProjectedText, ProjectedTextLayout, ProjectedTextRun, StreamNode,
    StreamOffset, StreamRange, StreamRevision, StreamSnapshot, StreamView, StreamingSource,
};
use crate::transcript::markdown::{
    AssistantContinuation, AssistantDocument, AssistantRowLayout, parse_assistant,
    parse_assistant_tail,
};
use crate::transcript::semantic::{
    AssistantSegment, SegmentKind, slice_segments, think_to_text_newline,
};

#[derive(Debug)]
pub(crate) struct AssistantStream {
    segments: Vec<AssistantSegment>,
    source_base: StreamOffset,
    source_end: StreamOffset,
    continuation: Option<AssistantContinuation>,
    revision: StreamRevision,
    sealed: bool,
}

impl AssistantStream {
    pub(crate) fn new() -> Self {
        Self {
            segments: Vec::new(),
            source_base: StreamOffset::ZERO,
            source_end: StreamOffset::ZERO,
            continuation: None,
            revision: StreamRevision::ZERO,
            sealed: false,
        }
    }

    pub(crate) fn push_delta(&mut self, kind: SegmentKind, chunk: &str) {
        assert!(!self.sealed, "cannot append to sealed AssistantStream");
        if chunk.is_empty() {
            return;
        }

        let chunk = think_to_text_newline(&self.segments, kind, chunk);
        let chunk_len = chunk.len() as u64;

        match kind {
            SegmentKind::Text => {
                if let Some(AssistantSegment::Text(text)) = self.segments.last_mut() {
                    text.push_str(&chunk);
                } else {
                    self.segments
                        .push(AssistantSegment::Text(chunk.into_owned()));
                }
            }
            SegmentKind::Thinking => {
                if let Some(AssistantSegment::Thinking(text)) = self.segments.last_mut() {
                    text.push_str(&chunk);
                } else {
                    self.segments
                        .push(AssistantSegment::Thinking(chunk.into_owned()));
                }
            }
        }

        self.source_end = self.source_end.saturating_add(chunk_len);
        self.revision = self.revision.next();
    }

    pub(crate) fn segments(&self) -> &[AssistantSegment] {
        &self.segments
    }

    fn restart_plan_at(
        &self,
        offset: StreamOffset,
    ) -> (StreamOffset, Option<AssistantContinuation>) {
        let doc = parse_assistant(&self.segments);
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
        let (view, stable_through) =
            build_assistant_stream_view(&doc, self.source_base, self.source_end, self.sealed);

        StreamSnapshot {
            revision: self.revision,
            source_base: self.source_base,
            source_end: self.source_end,
            stable_through,
            view,
        }
    }

    fn compact_before(&mut self, offset: StreamOffset) {
        if offset <= self.source_base {
            return;
        }

        debug_assert!(offset <= self.source_end);

        let (restart, continuation) = self.restart_plan_at(offset);
        self.continuation = continuation;
        self.source_base = restart;
        self.revision = self.revision.next();
    }

    fn seal(&mut self) {
        if self.sealed {
            return;
        }
        self.sealed = true;
        self.revision = self.revision.next();
    }

    fn is_sealed(&self) -> bool {
        self.sealed
    }
}

/// Builds the [`StreamView`] and computes the width-independent semantic stability frontier.
fn build_assistant_stream_view(
    doc: &AssistantDocument,
    source_base: StreamOffset,
    source_end: StreamOffset,
    sealed: bool,
) -> (StreamView, StreamOffset) {
    let mut nodes = Vec::new();
    let mut cursor = source_base;
    let mut stable_through = source_base;

    for row in &doc.rows {
        let owned_end;

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
        if let Some((body_start, body_column, prefix, show_prefix)) = list_layout {
            if show_prefix {
                debug_assert_eq!(
                    UnicodeWidthStr::width(prefix.as_str()),
                    usize::from(body_column)
                );
            }
            let runs = row
                .projected_runs
                .iter()
                .map(|run| ProjectedTextRun {
                    display: run.display.clone(),
                    style: run.style.clone(),
                    owned: StreamRange::new(
                        cursor.saturating_add(run.owned.start as u64),
                        cursor.saturating_add(run.owned.end as u64),
                    ),
                    exact_visible: run.exact_visible.as_ref().map(|visible| {
                        StreamRange::new(
                            cursor.saturating_add(visible.start as u64),
                            cursor.saturating_add(visible.end as u64),
                        )
                    }),
                })
                .collect();
            let projected = ProjectedText {
                content_range: StreamRange::new(
                    cursor,
                    cursor.saturating_add(row.source.content_len as u64),
                ),
                terminator: if row.source.has_newline {
                    ExactTerminator::HardNewline
                } else {
                    ExactTerminator::None
                },
                width: WidthRule::Fill,
                wrap: WrapMode::WordThenGrapheme,
                align: HorizontalAlign::Start,
                layout: ProjectedTextLayout::Hanging {
                    body_column,
                    prefix,
                    prefix_style: row.style.clone(),
                    prefix_source: StreamRange::new(
                        cursor,
                        cursor.saturating_add(body_start as u64),
                    ),
                    show_prefix,
                },
                runs,
            };
            let node = StreamNode::projected_text(projected);
            owned_end = node.owned_range().end;
            nodes.push(node);
        } else {
            let runs = if row.projected_runs.is_empty() {
                let mut run_cursor = cursor;
                row.spans
                    .iter()
                    .map(|span| {
                        let start = run_cursor;
                        run_cursor = run_cursor.saturating_add(span.text.len() as u64);
                        ProjectedTextRun {
                            display: span.text.clone(),
                            style: span.style.clone(),
                            owned: StreamRange::new(start, run_cursor),
                            exact_visible: Some(StreamRange::new(start, run_cursor)),
                        }
                    })
                    .collect()
            } else {
                row.projected_runs
                    .iter()
                    .map(|run| ProjectedTextRun {
                        display: run.display.clone(),
                        style: run.style.clone(),
                        owned: StreamRange::new(
                            cursor.saturating_add(run.owned.start as u64),
                            cursor.saturating_add(run.owned.end as u64),
                        ),
                        exact_visible: run.exact_visible.as_ref().map(|visible| {
                            StreamRange::new(
                                cursor.saturating_add(visible.start as u64),
                                cursor.saturating_add(visible.end as u64),
                            )
                        }),
                    })
                    .collect()
            };
            let projected = ProjectedText {
                content_range: text_range,
                terminator: if row.source.has_newline {
                    ExactTerminator::HardNewline
                } else {
                    ExactTerminator::None
                },
                width: WidthRule::Fill,
                wrap: WrapMode::WordThenGrapheme,
                align: HorizontalAlign::Start,
                layout: ProjectedTextLayout::Plain,
                runs,
            };
            let node = StreamNode::projected_text(projected);
            owned_end = node.owned_range().end;
            nodes.push(node);
        }

        // The parser's stable_prefix_len is already the truthful append-safe
        // source frontier: semantic stability intersected with source EGC
        // append safety. Provenance only controls commit granularity below.
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

    (StreamView::new(nodes), stable_through.min(source_end))
}

#[cfg(test)]
mod tests;

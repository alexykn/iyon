use ratatui::{
    style::{Color, Style},
    text::Line,
};

use crate::transcript::wrap::{TranscriptCommitBoundary, wrap_transcript_rows};

#[derive(Debug, Clone)]
pub(crate) enum TimelineItem {
    UserMessage { text: String },
    AssistantMessage { text: String },
    ErrorMessage { text: String },
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TuiFormatter;

impl TuiFormatter {
    pub(crate) fn format(&self, item: &TimelineItem) -> Vec<Line<'static>> {
        match item {
            TimelineItem::UserMessage { text } => {
                self.format_text_message(text, Style::default().bg(Color::Rgb(45, 55, 72)), true)
            }
            TimelineItem::AssistantMessage { text } => self.format_assistant_message(text, true),
            TimelineItem::ErrorMessage { text } => {
                self.format_text_message(text, Style::default().fg(Color::Red), true)
            }
        }
    }

    pub(crate) fn format_assistant_message(
        &self,
        text: &str,
        include_trailing_blank: bool,
    ) -> Vec<Line<'static>> {
        self.format_text_message(text, Style::default(), include_trailing_blank)
    }

    pub(crate) fn format_assistant_body(&self, text: &str) -> Vec<Line<'static>> {
        Self::format_text_body(text, Style::default())
    }

    fn format_text_message(
        &self,
        text: &str,
        style: Style,
        include_trailing_blank: bool,
    ) -> Vec<Line<'static>> {
        let mut rows = Vec::new();
        rows.push(Line::styled("", style));
        rows.extend(Self::format_text_body(text, style));
        if include_trailing_blank {
            rows.push(Line::styled("", style));
        }
        rows
    }

    fn format_text_body(text: &str, style: Style) -> Vec<Line<'static>> {
        styled_lines_preserving_newlines(text, style)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TranscriptState {
    canonical_items: Vec<TimelineItem>,
    logical_rows_cache: Vec<Line<'static>>,
    rendered_rows_cache: Option<TranscriptRenderCache>,
    commit_boundary: TranscriptCommitBoundary,
    formatter: TuiFormatter,
    assistant_stream_open: bool,
}

impl Default for TranscriptState {
    fn default() -> Self {
        Self {
            canonical_items: Vec::new(),
            logical_rows_cache: Vec::new(),
            rendered_rows_cache: None,
            commit_boundary: TranscriptCommitBoundary::default(),
            formatter: TuiFormatter,
            assistant_stream_open: false,
        }
    }
}

impl TranscriptState {
    pub(crate) fn push_item(&mut self, item: TimelineItem) {
        self.assistant_stream_open = false;
        self.canonical_items.push(item);
        self.rebuild_logical_rows_cache();
    }

    pub(crate) fn append_assistant_fragment(&mut self, text: String) {
        self.append_assistant_text(text, false);
    }

    pub(crate) fn append_assistant_stream_fragment(&mut self, text: String) {
        self.append_assistant_text(text, true);
    }

    pub(crate) fn finish_assistant_stream(&mut self) {
        if !self.assistant_stream_open {
            return;
        }
        self.assistant_stream_open = false;
        self.rebuild_logical_rows_cache();
    }

    fn append_assistant_text(&mut self, text: String, stream_open: bool) {
        if text.is_empty() {
            return;
        }

        match self.canonical_items.last_mut() {
            Some(TimelineItem::AssistantMessage { text: existing }) => existing.push_str(&text),
            _ => self
                .canonical_items
                .push(TimelineItem::AssistantMessage { text }),
        }
        self.assistant_stream_open = stream_open;
        self.rebuild_logical_rows_cache();
    }

    fn rebuild_logical_rows_cache(&mut self) {
        self.logical_rows_cache.clear();
        let open_assistant_idx = self.assistant_stream_open.then(|| {
            self.canonical_items
                .iter()
                .rposition(|item| matches!(item, TimelineItem::AssistantMessage { .. }))
        });
        for (idx, item) in self.canonical_items.iter().enumerate() {
            match item {
                TimelineItem::AssistantMessage { text }
                    if open_assistant_idx == Some(Some(idx)) =>
                {
                    // While an assistant stream is open, the transcript owns only the
                    // frozen/spilled portion of that assistant message. Keep the normal
                    // message top padding so the gap from the preceding user message is
                    // stable, but omit the official bottom padding: the live active pane
                    // owns the active/input margin until the stream is finalized.
                    let mut rows = self.formatter.format_assistant_message(text, false);

                    // The frozen transcript and live active pane touch at this boundary.
                    // If the spilled fragment currently ends in an empty rendered row,
                    // suppress exactly one such row so the seam does not show a moving
                    // blank gap while streaming. Finalized messages are rebuilt through
                    // the normal formatter path and regain their bottom padding.
                    drop_trailing_empty_row(&mut rows);
                    self.logical_rows_cache.extend(rows);
                }
                _ => self.logical_rows_cache.extend(self.formatter.format(item)),
            }
        }
        self.rendered_rows_cache = None;
    }

    pub(crate) fn ensure_render_cache(&mut self, width: u16) {
        let should_recompute = match self.rendered_rows_cache.as_ref() {
            Some(cache) => cache.width != width,
            None => true,
        };

        if !should_recompute {
            return;
        }

        let cache = TranscriptRenderCache::from_logical_rows(
            width.max(1),
            &self.logical_rows_cache,
            self.commit_boundary,
        );
        self.rendered_rows_cache = Some(cache);
    }

    pub(crate) fn uncommitted_rows(&self) -> &[Line<'static>] {
        let Some(cache) = self.rendered_rows_cache.as_ref() else {
            return &[];
        };

        &cache.rows
    }

    pub(crate) fn uncommitted_len(&self) -> usize {
        self.uncommitted_rows().len()
    }

    pub(crate) fn mark_rows_committed(&mut self, rows: usize) {
        let Some(cache) = self.rendered_rows_cache.as_mut() else {
            return;
        };

        let committed_rows = rows.min(cache.rows.len());
        if committed_rows == 0 {
            return;
        }

        self.commit_boundary = cache.row_end_boundaries[committed_rows - 1];
        cache.rows.drain(..committed_rows);
        cache.row_end_boundaries.drain(..committed_rows);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TranscriptRenderCache {
    width: u16,
    rows: Vec<Line<'static>>,
    row_end_boundaries: Vec<TranscriptCommitBoundary>,
}

impl TranscriptRenderCache {
    fn from_logical_rows(
        width: u16,
        logical_rows: &[Line<'static>],
        commit_boundary: TranscriptCommitBoundary,
    ) -> Self {
        let wrapped = wrap_transcript_rows(width, logical_rows, commit_boundary);

        Self {
            width,
            rows: wrapped.rows,
            row_end_boundaries: wrapped.row_end_boundaries,
        }
    }
}

fn styled_lines_preserving_newlines(text: &str, style: Style) -> Vec<Line<'static>> {
    text.split('\n')
        .map(|line| Line::styled(line.to_string(), style))
        .collect()
}

fn drop_trailing_empty_row(rows: &mut Vec<Line<'static>>) {
    let Some(last) = rows.last() else {
        return;
    };

    let is_empty = last
        .spans
        .iter()
        .all(|span| span.content.as_ref().is_empty());
    if is_empty {
        rows.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_text(line: &Line<'static>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    }

    #[test]
    fn formatter_preserves_multiline_message_shape() {
        let formatter = TuiFormatter;
        let item = TimelineItem::UserMessage {
            text: "line1\nline2\n".to_string(),
        };

        let lines = formatter.format(&item);
        let text_rows = lines.iter().map(line_text).collect::<Vec<_>>();

        assert_eq!(text_rows, vec!["", "line1", "line2", "", ""]);
    }

    #[test]
    fn wraps_transcript_rows_to_visual_width() {
        let mut transcript = TranscriptState::default();
        transcript.push_item(TimelineItem::AssistantMessage {
            text: "abcdef".to_string(),
        });

        transcript.ensure_render_cache(3);

        let rows = transcript
            .uncommitted_rows()
            .iter()
            .map(line_text)
            .collect::<Vec<_>>();
        assert_eq!(rows, ["", "abc", "def", ""]);
    }

    #[test]
    fn commit_boundary_survives_resize_after_partial_line_commit() {
        let mut transcript = TranscriptState::default();
        transcript.push_item(TimelineItem::AssistantMessage {
            text: "abcdef".to_string(),
        });

        transcript.ensure_render_cache(3);
        transcript.mark_rows_committed(2);
        transcript.ensure_render_cache(2);

        let rows = transcript
            .uncommitted_rows()
            .iter()
            .map(line_text)
            .collect::<Vec<_>>();
        assert_eq!(rows, ["de", "f", ""]);
    }
}

//! Text measurement, wrapping, and projected-text compilation.

use std::borrow::Cow;

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{
    physical::{PhysicalCell, PhysicalRow, PhysicalStyle},
    presentation::{
        StreamRange, WidthRule,
        ir::{TextCursorAnchor, TextView},
        stream::{ProjectedText, ProjectedTextLayout},
        wrap::{StyledGrapheme, WrappedLine, styled_hard_lines, wrap_styled_lines},
    },
};

use super::{CompiledTextRow, ViewCompiler};

impl ViewCompiler {
    pub(crate) fn compile_projected_text_with_metadata(
        &self,
        text: &ProjectedText,
        max_width: u16,
    ) -> (u16, Vec<CompiledTextRow>) {
        self.compile_projected_text_with_style(text, max_width, PhysicalStyle::default())
    }

    fn compile_projected_text_with_style(
        &self,
        text: &ProjectedText,
        max_width: u16,
        inherited: PhysicalStyle,
    ) -> (u16, Vec<CompiledTextRow>) {
        if let ProjectedTextLayout::Hanging {
            body_column,
            prefix,
            prefix_style,
            show_prefix,
            ..
        } = &text.layout
        {
            let body_start = text
                .runs
                .first()
                .map_or(text.content_range.end, |run| run.owned.start);
            let body = ProjectedText {
                content_range: StreamRange::new(body_start, text.content_range.end),
                terminator: text.terminator,
                width: WidthRule::Fill,
                wrap: text.wrap,
                align: text.align,
                layout: ProjectedTextLayout::Plain,
                runs: text.runs.clone(),
            };
            let body_width = max_width.saturating_sub(*body_column).max(1);
            let (_, body_rows) =
                self.compile_projected_text_with_style(&body, body_width, inherited);
            let prefix_width = UnicodeWidthStr::width(prefix.as_str());
            let mut rows = Vec::with_capacity(body_rows.len());
            for (index, mut row) in body_rows.into_iter().enumerate() {
                let indent = if index == 0 && *show_prefix {
                    prefix.clone()
                } else {
                    " ".repeat(usize::from(*body_column))
                };
                let prefix_style = if index == 0 && *show_prefix {
                    self.theme.resolve_text_style(inherited, prefix_style)
                } else {
                    inherited
                };
                let prefix_row = row_from_string(&indent, prefix_style);
                let mut cells = prefix_row.cells().to_vec();
                cells.extend_from_slice(row.row.cells());
                row.row = PhysicalRow::from_cells(cells);
                row.width = row.width.saturating_add(if index == 0 && *show_prefix {
                    prefix_width
                } else {
                    usize::from(*body_column)
                });
                row.fits = row.width <= usize::from(max_width);
                row.source_end = row.source_end.map(|end| {
                    end + (body_start.as_u64() - text.content_range.start.as_u64()) as usize
                });
                rows.push(row);
            }
            return (max_width, rows);
        }

        let hard_lines = projected_hard_lines(&self.theme, text, inherited);
        let intrinsic_width = hard_lines
            .iter()
            .map(|line| line.iter().map(|atom| atom.width).sum::<usize>())
            .max()
            .unwrap_or(0);
        let width = match text.width {
            WidthRule::Fit => intrinsic_width.min(usize::from(max_width)) as u16,
            WidthRule::Fill => max_width,
        };
        let rows = wrap_styled_lines(&hard_lines, width, text.wrap)
            .into_iter()
            .map(|w_line| CompiledTextRow {
                row: row_from_graphemes(&w_line.graphemes),
                source_end: w_line
                    .graphemes
                    .last()
                    .and_then(|g| g.source.as_ref())
                    .map(|r| r.end),
                fits: w_line.fits,
                width: w_line.width,
            })
            .collect();
        (width, rows)
    }

    pub(crate) fn compile_text_with_metadata(
        &self,
        text: &TextView,
        max_width: u16,
        width_rule: WidthRule,
        inherited: PhysicalStyle,
        track_source: bool,
    ) -> (u16, Vec<CompiledTextRow>) {
        let mut relative_source = 0usize;
        let spans = text.spans.iter().map(|span| {
            let base = if track_source {
                let current = relative_source;
                relative_source += span.text.len();
                Some(current)
            } else {
                None
            };
            (
                span.text.as_str(),
                self.theme.resolve_text_style(inherited, &span.style),
                base,
            )
        });
        let hard_lines = styled_hard_lines(spans);
        let intrinsic_width = hard_lines
            .iter()
            .map(|line| line.iter().map(|grapheme| grapheme.width).sum::<usize>())
            .max()
            .unwrap_or(0);
        let source = text.cursor.map(|anchor| {
            let source = text
                .spans
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>();
            validate_cursor_anchor(&source, anchor);
            source
        });
        let cursor_needs_cell = text.cursor.is_some_and(|anchor| {
            !hard_lines.iter().flatten().any(|grapheme| {
                grapheme.source.as_ref().is_some_and(|range| {
                    range.start <= anchor.byte_offset && anchor.byte_offset < range.end
                })
            })
        });
        let intrinsic_width = intrinsic_width + usize::from(cursor_needs_cell);
        let width = match width_rule {
            WidthRule::Fit => intrinsic_width.min(usize::from(max_width)) as u16,
            WidthRule::Fill => max_width,
        };
        let mut wrapped = if let Some(source) = source.as_deref()
            && text.cursor.is_some()
        {
            crate::presentation::wrap::wrap_input_styled_lines(&hard_lines, source, width)
        } else {
            wrap_styled_lines(&hard_lines, width, text.wrap)
        };
        if let (Some(anchor), Some(source)) = (text.cursor, source.as_deref()) {
            apply_cursor_anchor(&mut wrapped, source, anchor, inherited, usize::from(width));
        }
        let rows = wrapped
            .into_iter()
            .map(|w_line| CompiledTextRow {
                row: row_from_graphemes(&w_line.graphemes),
                source_end: w_line
                    .graphemes
                    .last()
                    .and_then(|g| g.source.as_ref())
                    .map(|r| r.end),
                fits: w_line.fits,
                width: w_line.width,
            })
            .collect();
        (width, rows)
    }
}

fn validate_cursor_anchor(text: &str, anchor: TextCursorAnchor) {
    assert!(
        anchor.byte_offset <= text.len(),
        "text cursor anchor exceeds source length"
    );
    assert!(
        text.is_char_boundary(anchor.byte_offset),
        "text cursor anchor is not a UTF-8 boundary"
    );
}

fn apply_cursor_anchor(
    rows: &mut Vec<WrappedLine<'_>>,
    source: &str,
    anchor: TextCursorAnchor,
    inherited: PhysicalStyle,
    target_width: usize,
) {
    for row in rows.iter_mut() {
        if let Some(grapheme) = row.graphemes.iter_mut().find(|grapheme| {
            grapheme.source.as_ref().is_some_and(|range| {
                range.start <= anchor.byte_offset && anchor.byte_offset < range.end
            })
        }) {
            grapheme.style.reversed = true;
            return;
        }
    }

    let target = if anchor.byte_offset > 0 && source[..anchor.byte_offset].ends_with('\n') {
        rows.iter()
            .position(|row| {
                row.graphemes.iter().any(|grapheme| {
                    grapheme
                        .source
                        .as_ref()
                        .is_some_and(|range| range.start >= anchor.byte_offset)
                })
            })
            .unwrap_or_else(|| rows.len().saturating_sub(1))
    } else {
        rows.iter()
            .rposition(|row| {
                row.graphemes.iter().any(|grapheme| {
                    grapheme
                        .source
                        .as_ref()
                        .is_some_and(|range| range.end <= anchor.byte_offset)
                })
            })
            .unwrap_or(0)
    };
    let row = rows.get_mut(target).expect("text cursor requires a row");
    let style = row
        .graphemes
        .last()
        .map_or(inherited, |grapheme| grapheme.style);
    let cursor = StyledGrapheme {
        text: Cow::Owned(" ".to_string()),
        width: 1,
        style: PhysicalStyle {
            reversed: true,
            ..style
        },
        source: None,
    };
    if row.width.saturating_add(1) > target_width && row.width > 0 {
        rows.push(WrappedLine::new(vec![cursor], target_width));
    } else {
        row.graphemes.push(cursor);
        row.width = row.width.saturating_add(1);
        row.fits = row.width <= target_width;
    }
}

fn row_from_string(text: &str, style: PhysicalStyle) -> PhysicalRow {
    let graphemes = text
        .graphemes(true)
        .map(|text| StyledGrapheme {
            text: Cow::Owned(text.to_string()),
            width: UnicodeWidthStr::width(text),
            style,
            source: None,
        })
        .collect::<Vec<_>>();
    row_from_graphemes(&graphemes)
}

fn row_from_graphemes(graphemes: &[StyledGrapheme<'_>]) -> PhysicalRow {
    let mut cells = Vec::new();
    for grapheme in graphemes {
        if grapheme.width == 0 {
            continue;
        }
        cells.push(PhysicalCell {
            grapheme: Some(grapheme.text.to_string()),
            style: grapheme.style,
            painted: true,
            continuation: false,
        });
        for _ in 1..grapheme.width {
            cells.push(PhysicalCell {
                grapheme: None,
                style: grapheme.style,
                painted: true,
                continuation: true,
            });
        }
    }
    PhysicalRow::from_cells(cells)
}

#[derive(Debug, Clone)]
struct ProjectedExactFragment {
    display_start: usize,
    display_end: usize,
    style: PhysicalStyle,
    owned: StreamRange,
    visible: StreamRange,
}

/// Tokenizes adjacent exact projected runs as one visible string so an EGC
/// cannot be split merely because Markdown/style provenance changes at a run
/// boundary. Replacement runs remain explicit indivisible barriers.
fn projected_hard_lines(
    theme: &super::ThemeResolver,
    text: &ProjectedText,
    inherited: PhysicalStyle,
) -> Vec<Vec<StyledGrapheme<'static>>> {
    let mut hard_lines = vec![Vec::new()];
    let mut exact_display = String::new();
    let mut exact_fragments = Vec::new();

    for run in &text.runs {
        if run.display.is_empty() {
            continue;
        }
        let style = theme.resolve_text_style(inherited, &run.style);
        let Some(visible) = run.exact_visible else {
            append_projected_exact(
                &mut hard_lines,
                &exact_display,
                &exact_fragments,
                text.content_range.start,
            );
            exact_display.clear();
            exact_fragments.clear();
            hard_lines.last_mut().unwrap().push(StyledGrapheme {
                text: Cow::Owned(run.display.clone()),
                width: UnicodeWidthStr::width(run.display.as_str()),
                style,
                source: Some(
                    (run.owned.start.as_u64() - text.content_range.start.as_u64()) as usize
                        ..(run.owned.end.as_u64() - text.content_range.start.as_u64()) as usize,
                ),
            });
            continue;
        };

        let display_start = exact_display.len();
        exact_display.push_str(&run.display);
        exact_fragments.push(ProjectedExactFragment {
            display_start,
            display_end: exact_display.len(),
            style,
            owned: run.owned,
            visible,
        });
    }

    append_projected_exact(
        &mut hard_lines,
        &exact_display,
        &exact_fragments,
        text.content_range.start,
    );
    hard_lines
}

fn append_projected_exact(
    hard_lines: &mut Vec<Vec<StyledGrapheme<'static>>>,
    display: &str,
    fragments: &[ProjectedExactFragment],
    content_start: crate::presentation::StreamOffset,
) {
    for (relative, grapheme) in display.grapheme_indices(true) {
        let relative_end = relative + grapheme.len();
        let first = fragments
            .iter()
            .find(|fragment| {
                relative < fragment.display_end && relative_end > fragment.display_start
            })
            .expect("projected EGC must overlap an exact fragment");
        let last = fragments
            .iter()
            .rev()
            .find(|fragment| fragment.display_start < relative_end)
            .expect("projected EGC must overlap an exact fragment");

        let first_offset = relative - first.display_start;
        let source_start = if first_offset == 0 {
            first.owned.start
        } else {
            first.visible.start.saturating_add(first_offset as u64)
        };
        let last_offset_end = relative_end - last.display_start;
        let last_display_len = last.display_end - last.display_start;
        let source_end = if last_offset_end == last_display_len {
            last.owned.end
        } else {
            last.visible.start.saturating_add(last_offset_end as u64)
        };
        let mapped = StyledGrapheme {
            text: Cow::Owned(grapheme.to_string()),
            width: UnicodeWidthStr::width(grapheme),
            style: first.style,
            source: Some(
                (source_start.as_u64() - content_start.as_u64()) as usize
                    ..(source_end.as_u64() - content_start.as_u64()) as usize,
            ),
        };
        if grapheme == "\n" {
            hard_lines.push(Vec::new());
        } else {
            hard_lines.last_mut().unwrap().push(mapped);
        }
    }
}

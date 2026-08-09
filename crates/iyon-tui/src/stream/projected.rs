//! Width-independent projected stream text.

use unicode_segmentation::UnicodeSegmentation;

use crate::presentation::{HorizontalAlign, StyleSpec, TextSpan, WidthRule, WrapMode};

use super::coord::{StreamOffset, StreamRange};

/// Structural terminator owned by a projected text node after its visible text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ExactTerminator {
    #[default]
    None,
    HardNewline,
}

impl ExactTerminator {
    pub(crate) const fn source_len(self) -> u64 {
        match self {
            Self::None => 0,
            Self::HardNewline => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProjectedText {
    pub(crate) content_range: StreamRange,
    pub(crate) terminator: ExactTerminator,
    pub(crate) width: WidthRule,
    pub(crate) wrap: WrapMode,
    pub(crate) align: HorizontalAlign,
    pub(crate) layout: ProjectedTextLayout,
    pub(crate) runs: Vec<ProjectedTextRun>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ProjectedTextLayout {
    Plain,
    Hanging {
        body_column: u16,
        prefix: String,
        prefix_style: StyleSpec,
        prefix_source: StreamRange,
        show_prefix: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProjectedTextRun {
    pub(crate) display: String,
    pub(crate) style: StyleSpec,
    pub(crate) owned: StreamRange,
    pub(crate) exact_visible: Option<StreamRange>,
}

impl ProjectedText {
    pub(crate) fn owned_range(&self) -> StreamRange {
        StreamRange::new(
            self.content_range.start,
            self.content_range
                .end
                .saturating_add(self.terminator.source_len()),
        )
    }

    pub(crate) fn identity(
        content_range: StreamRange,
        terminator: ExactTerminator,
        spans: Vec<TextSpan>,
    ) -> Self {
        let mut cursor = content_range.start;
        let mut runs: Vec<ProjectedTextRun> = Vec::new();
        for span in spans {
            if span.text.is_empty() {
                continue;
            }
            let start = cursor;
            cursor = cursor.saturating_add(span.text.len() as u64);
            if let Some(previous) = runs.last_mut()
                && previous.style == span.style
                && previous.owned.end == start
            {
                previous.display.push_str(&span.text);
                previous.owned.end = cursor;
                previous.exact_visible = Some(StreamRange::new(previous.owned.start, cursor));
            } else {
                runs.push(ProjectedTextRun {
                    display: span.text,
                    style: span.style,
                    owned: StreamRange::new(start, cursor),
                    exact_visible: Some(StreamRange::new(start, cursor)),
                });
            }
        }
        Self {
            content_range,
            terminator,
            width: WidthRule::Fit,
            wrap: WrapMode::WordThenGrapheme,
            align: HorizontalAlign::Start,
            layout: ProjectedTextLayout::Plain,
            runs,
        }
    }
}

pub(crate) fn slice_projected_text(text: &ProjectedText, offset: StreamOffset) -> ProjectedText {
    assert!(offset > text.content_range.start);
    assert!(offset < text.owned_range().end);
    assert!(
        offset <= text.content_range.end,
        "cannot slice inside a terminator"
    );
    assert!(
        projected_checkpoint_is_legal(text, offset),
        "cannot slice inside a projected EGC"
    );

    let mut runs = Vec::new();
    for run in &text.runs {
        if run.owned.end <= offset {
            continue;
        }
        if run.owned.start >= offset {
            runs.push(run.clone());
            continue;
        }

        let Some(visible) = run.exact_visible else {
            panic!("projected replacement may only be sliced at run boundaries");
        };
        assert!(offset >= visible.start && offset <= visible.end);
        let relative = offset.as_u64().saturating_sub(visible.start.as_u64()) as usize;
        assert!(run.display.is_char_boundary(relative));
        assert!(
            run.display
                .grapheme_indices(true)
                .any(|(start, _)| start == relative)
        );
        let display = run.display[relative..].to_string();
        runs.push(ProjectedTextRun {
            display,
            style: run.style.clone(),
            owned: StreamRange::new(offset, run.owned.end),
            exact_visible: Some(StreamRange::new(offset, visible.end)),
        });
    }

    ProjectedText {
        content_range: StreamRange::new(offset, text.content_range.end),
        terminator: text.terminator,
        width: text.width,
        wrap: text.wrap,
        align: text.align,
        layout: match &text.layout {
            ProjectedTextLayout::Plain => ProjectedTextLayout::Plain,
            ProjectedTextLayout::Hanging {
                body_column,
                prefix,
                prefix_style,
                prefix_source,
                ..
            } => ProjectedTextLayout::Hanging {
                body_column: *body_column,
                prefix: prefix.clone(),
                prefix_style: prefix_style.clone(),
                prefix_source: *prefix_source,
                show_prefix: offset <= prefix_source.start,
            },
        },
        runs,
    }
}

/// One visible grapheme together with the source it truthfully consumes.
/// `run_index` identifies the first exact run contributing to the grapheme;
/// `None` marks a replacement barrier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectedAtom {
    pub(crate) display: String,
    pub(crate) owned: StreamRange,
    pub(crate) style: StyleSpec,
    pub(crate) run_index: Option<usize>,
    pub(crate) barriers: Vec<StreamOffset>,
}

/// Concatenates adjacent exact-visible runs before EGC segmentation. This is
/// the shared barrier model for compilation and source checkpoint validation.
pub(crate) fn projected_atoms(text: &ProjectedText) -> Vec<ProjectedAtom> {
    let mut atoms = Vec::new();
    let mut exact_display = String::new();
    let mut fragments = Vec::new();

    let flush_exact =
        |atoms: &mut Vec<ProjectedAtom>,
         display: &mut String,
         fragments: &mut Vec<(usize, usize, StreamRange, StreamRange, usize, StyleSpec)>| {
            for (relative, grapheme) in display.grapheme_indices(true) {
                let relative_end = relative + grapheme.len();
                let contributors = fragments
                    .iter()
                    .enumerate()
                    .filter(|(_, (start, end, _, _, _, _))| {
                        relative < *end && relative_end > *start
                    })
                    .collect::<Vec<_>>();
                let first = contributors
                    .first()
                    .map(|(_, fragment)| fragment)
                    .expect("projected EGC must overlap an exact fragment");
                let last = contributors
                    .last()
                    .map(|(_, fragment)| fragment)
                    .expect("projected EGC must overlap an exact fragment");
                let first_offset = relative - first.0;
                let source_start = if first_offset == 0 {
                    first.2.start
                } else {
                    first.3.start.saturating_add(first_offset as u64)
                };
                let last_offset_end = relative_end - last.0;
                let last_display_len = last.1 - last.0;
                let source_end = if last_offset_end == last_display_len {
                    last.2.end
                } else {
                    last.3.start.saturating_add(last_offset_end as u64)
                };
                atoms.push(ProjectedAtom {
                    display: grapheme.to_owned(),
                    owned: StreamRange::new(source_start, source_end),
                    style: first.5.clone(),
                    run_index: Some(first.4),
                    barriers: contributors
                        .iter()
                        .take(contributors.len().saturating_sub(1))
                        .map(|(_, fragment)| fragment.2.end)
                        .collect(),
                });
            }
            display.clear();
            fragments.clear();
        };

    for (run_index, run) in text.runs.iter().enumerate() {
        if run.display.is_empty() {
            continue;
        }
        let Some(visible) = run.exact_visible else {
            flush_exact(&mut atoms, &mut exact_display, &mut fragments);
            atoms.push(ProjectedAtom {
                display: run.display.clone(),
                owned: run.owned,
                style: run.style.clone(),
                run_index: None,
                barriers: Vec::new(),
            });
            continue;
        };
        let display_start = exact_display.len();
        exact_display.push_str(&run.display);
        fragments.push((
            display_start,
            exact_display.len(),
            run.owned,
            visible,
            run_index,
            run.style.clone(),
        ));
    }
    flush_exact(&mut atoms, &mut exact_display, &mut fragments);
    atoms
}

pub(crate) fn projected_checkpoint_is_legal(text: &ProjectedText, offset: StreamOffset) -> bool {
    if offset <= text.content_range.start || offset >= text.content_range.end {
        return true;
    }
    !projected_atoms(text)
        .iter()
        .any(|atom| atom.barriers.contains(&offset))
}

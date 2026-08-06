//! Transcript logical-row model.
//!
//! A transcript is a sequence of logical rows, each with a *layout* describing
//! its geometry: an outer viewport margin, an optional marker (hanging gutter),
//! and a structural nesting indentation. Layout is declarative metadata; the
//! wrap step derives concrete first/continuation prefixes from it once.
//!
//! The invariant (documented, because it is *not* a single shared content
//! column):
//!
//! > Every row begins at a shared outer baseline. Structural nesting and marker
//! > gutters add explicit indentation relative to that baseline.
//!
//! ```text
//! content_column = outer.left + nesting_depth * LIST_INDENT + marker_gutter_width
//! ```
//!
//! So plain agent text, top-level lists, nested lists, and tool calls land at
//! different columns on purpose: markers hang in a gutter and content is a
//! stable edge, but nesting indents relative to the outer baseline.

use ratatui::style::Style;
use ratatui::text::Line;
use unicode_width::UnicodeWidthStr;

/// Outer viewport gutter (symmetric for transcript content). Rows render within
/// `terminal_width - outer.left - outer.right`.
pub(crate) const OUTER_MARGIN: usize = 2;

/// Structural indentation per nesting level (one logical tab).
pub(crate) const LIST_INDENT: usize = 2;

/// Symmetric left/right insets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Insets {
    pub left: usize,
    pub right: usize,
}

impl Insets {
    pub const ZERO: Insets = Insets { left: 0, right: 0 };

    pub const fn symmetric(value: usize) -> Self {
        Self {
            left: value,
            right: value,
        }
    }
}

/// A hanging marker that occupies the gutter to the left of content. Every marker
/// renders as a fixed-width prefix (marker + trailing space), so the content edge
/// stays stable across wrapped continuation lines and across marker variants.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Marker {
    /// `• ` — markdown bullet.
    Bullet,
    /// `● ` — tool call.
    ToolCall,
    /// Right-aligned `9. ` / `10. ` ordered item. `index` is this row's item
    /// number; `digit_width` is the shared width of the widest item in the list
    /// (set by the list renderer), so single- and multi-digit numbers align.
    Ordered { index: usize, digit_width: usize },
}

impl Marker {
    /// The marker's visible text (marker + one separating space).
    pub(crate) fn text(&self) -> String {
        match self {
            Marker::Bullet => "• ".to_string(),
            Marker::ToolCall => "● ".to_string(),
            Marker::Ordered { index, digit_width } => {
                format!("{index:>width$}. ", width = *digit_width.min(&16))
            }
        }
    }

    /// Display width of [`Marker::text`].
    pub(crate) fn width(&self) -> usize {
        self.text().width()
    }
}

/// Declarative geometry for a row. Style is presentation and lives on the row /
/// spans, not here.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RowLayout {
    pub outer: Insets,
    pub nesting_depth: usize,
    pub marker: Option<Marker>,
    /// How many gutter columns are reserved for a marker, regardless of whether
    /// one is rendered. When `marker` is `Some`, this equals the marker's width
    /// (kept in sync by constructors); when `None` it reserves blank gutter so
    /// content aligns with a sibling marked row (e.g. tool results under tool
    /// calls).
    pub marker_gutter_width: usize,
}

impl RowLayout {
    /// Column 0 with no gutter, nesting, or marker (blank rows, user bubbles).
    pub(crate) const fn zero() -> Self {
        Self {
            outer: Insets::ZERO,
            nesting_depth: 0,
            marker: None,
            marker_gutter_width: 0,
        }
    }

    /// Standard symmetric outer margin, no marker or nesting.
    pub(crate) const fn content() -> Self {
        Self {
            outer: Insets::symmetric(OUTER_MARGIN),
            nesting_depth: 0,
            marker: None,
            marker_gutter_width: 0,
        }
    }

    /// The structural gutter width actually applied (the reserved gutter).
    #[cfg(test)]
    pub(crate) fn gutter_width(&self) -> usize {
        self.marker
            .as_ref()
            .map(Marker::width)
            .unwrap_or(self.marker_gutter_width)
    }

    /// The column where content begins (test/geometry helper): outer baseline +
    /// nesting + reserved gutter.
    #[cfg(test)]
    pub(crate) fn content_column(&self) -> usize {
        self.outer.left + self.nesting_depth * LIST_INDENT + self.gutter_width()
    }
}

/// A single logical row: display content plus declarative layout metadata.
#[derive(Debug, Clone)]
pub(crate) struct TranscriptRow {
    pub(crate) line: Line<'static>,
    /// Style applied to the left gutter/prefix (merged into the first span when
    /// it shares the style). Content styling lives in `line`'s spans.
    pub(crate) style: Style,
    pub(crate) layout: RowLayout,
}

impl TranscriptRow {
    /// A row at column 0 (blank rows, user bubbles).
    pub(crate) fn new(line: Line<'static>) -> Self {
        let style = line.style;
        Self {
            line,
            style,
            layout: RowLayout::zero(),
        }
    }

    /// An empty row (inter-block pacing), geometry-free at column 0.
    pub(crate) fn blank() -> Self {
        Self::new(Line::from(""))
    }

    /// A body/content row with the standard symmetric outer margin and no marker.
    pub(crate) fn content(line: Line<'static>, style: Style) -> Self {
        Self {
            line,
            style,
            layout: RowLayout::content(),
        }
    }

    /// A body row with the standard outer margin, from owned text.
    #[cfg(test)]
    pub(crate) fn body(text: impl Into<String>, style: Style) -> Self {
        Self::content(Line::styled(text.into(), style), style)
    }

    /// A tool-call bullet row: `● ` hangs in the gutter; text starts after it.
    pub(crate) fn bullet(text: impl Into<String>, style: Style) -> Self {
        let marker = Marker::ToolCall;
        Self {
            line: Line::styled(text.into(), style),
            style,
            layout: RowLayout {
                outer: Insets::symmetric(OUTER_MARGIN),
                nesting_depth: 0,
                marker: Some(marker.clone()),
                marker_gutter_width: marker.width(),
            },
        }
    }

    /// A tool-result body row: no marker, but it reserves the tool-call gutter so
    /// result text aligns vertically with the tool-call text above it (both at
    /// column `OUTER_MARGIN + marker_gutter_width`).
    pub(crate) fn tool_result(text: impl Into<String>, style: Style) -> Self {
        let gutter = Marker::ToolCall.width();
        Self {
            line: Line::styled(text.into(), style),
            style,
            layout: RowLayout {
                outer: Insets::symmetric(OUTER_MARGIN),
                nesting_depth: 0,
                marker: None,
                marker_gutter_width: gutter,
            },
        }
    }

    /// A markdown bullet list item at `depth` (0 = top level).
    pub(crate) fn markdown_unordered(line: Line<'static>, style: Style, depth: usize) -> Self {
        let marker = Marker::Bullet;
        Self {
            line,
            style,
            layout: RowLayout {
                outer: Insets::symmetric(OUTER_MARGIN),
                nesting_depth: depth,
                marker: Some(marker.clone()),
                marker_gutter_width: marker.width(),
            },
        }
    }

    /// A markdown ordered list item at `depth`, right-aligned within `digit_width`.
    pub(crate) fn markdown_ordered(
        line: Line<'static>,
        style: Style,
        index: usize,
        digit_width: usize,
        depth: usize,
    ) -> Self {
        let marker = Marker::Ordered { index, digit_width };
        Self {
            line,
            style,
            layout: RowLayout {
                outer: Insets::symmetric(OUTER_MARGIN),
                nesting_depth: depth,
                marker: Some(marker.clone()),
                marker_gutter_width: marker.width(),
            },
        }
    }
}

/// Precomputed, display-width-correct prefixes a row needs to wrap with a stable
/// hanging indent. `content_width` already accounts for the full left prefix and
/// the right margin, so wrapped physical rows never exceed the viewport.
#[derive(Debug, Clone)]
pub(crate) struct ComputedIndent {
    pub(crate) first_prefix: String,
    pub(crate) continuation_prefix: String,
    pub(crate) content_width: usize,
}

/// Derives the wrap prefixes and content width for a layout.
///
/// ```text
/// content_width = viewport_width
///     .saturating_sub(first_prefix_width)
///     .saturating_sub(outer.right)
/// ```
pub(crate) fn compute_indent(layout: &RowLayout, viewport_width: usize) -> ComputedIndent {
    let nesting = " ".repeat(layout.nesting_depth * LIST_INDENT);

    // The marker area: rendered marker text if a marker is present, plus any
    // reserved (blank) gutter when it is not (tool results align under tool calls).
    let marker_text = layout.marker.as_ref().map(Marker::text).unwrap_or_default();
    let reserved_gutter = layout.marker_gutter_width.max(marker_text.width());

    // First line: outer margin + nesting + marker, padded so content lands at the
    // reserved gutter column when there is no visible marker.
    let marker_padding = reserved_gutter.saturating_sub(marker_text.width());
    let first_prefix = format!(
        "{}{nesting}{marker_text}{}",
        " ".repeat(layout.outer.left),
        " ".repeat(marker_padding)
    );

    // Continuation lines: outer margin + nesting + a full reserved-gutter pad.
    let continuation_prefix = format!(
        "{}{}",
        " ".repeat(layout.outer.left),
        " ".repeat(nesting.width() + reserved_gutter)
    );

    debug_assert_eq!(
        first_prefix.width(),
        continuation_prefix.width(),
        "first and continuation prefixes must share display width"
    );

    let content_width = viewport_width
        .saturating_sub(first_prefix.width())
        .saturating_sub(layout.outer.right);

    ComputedIndent {
        first_prefix,
        continuation_prefix,
        content_width,
    }
}

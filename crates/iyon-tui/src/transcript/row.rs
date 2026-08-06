//! Transcript logical-row model.
//!
//! A transcript is a sequence of logical rows, each of which may carry a styled
//! *hanging indent* (a left margin). Indentation is a first-class field here
//! rather than literal spaces baked into span content, so the wrap step can
//! re-apply the margin to every wrapped physical row and wrap within the
//! remaining content width instead of counting the indent against it.

use ratatui::style::Style;
use ratatui::text::Line;

/// A styled left margin for a logical row, re-applied to every wrapped physical
/// row that logical row produces. Width is in display columns; `style` is the
/// style of the margin span (normally the same as the row's content style so
/// the indent reads as part of the line).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LeftMargin {
    pub(crate) width: u16,
    pub(crate) style: Style,
}

impl LeftMargin {
    /// The standard indent for tool-result title/body rows.
    pub(crate) const INDENT: u16 = 2;

    /// No indent (column 0).
    pub(crate) const ZERO: LeftMargin = LeftMargin::new(0, Style::reset());

    pub(crate) const fn new(width: u16, style: Style) -> Self {
        Self { width, style }
    }
}

/// A single logical row of the transcript: display content plus an optional
/// styled hanging indent. Content excludes the indent; the margin is applied at
/// wrap time so every physical row (including wrapped continuation lines) keeps
/// the same left margin.
#[derive(Debug, Clone)]
pub(crate) struct TranscriptRow {
    pub(crate) line: Line<'static>,
    pub(crate) margin: LeftMargin,
}

impl TranscriptRow {
    /// A row at column 0 (no indent).
    pub(crate) fn new(line: Line<'static>) -> Self {
        Self { line, margin: LeftMargin::ZERO }
    }

    /// A row with an explicit hanging indent.
    pub(crate) fn with_margin(line: Line<'static>, margin: LeftMargin) -> Self {
        Self { line, margin }
    }

    /// An empty row (used for inter-block pacing).
    pub(crate) fn blank() -> Self {
        Self::new(Line::from(""))
    }

    /// A plain styled row at column 0.
    pub(crate) fn styled(text: impl Into<String>, style: Style) -> Self {
        Self::new(Line::styled(text.into(), style))
    }

    /// A row indented by [`LeftMargin::INDENT`] columns whose margin and content share `style`.
    pub(crate) fn indented(text: impl Into<String>, style: Style) -> Self {
        Self::with_margin(Line::styled(text.into(), style), LeftMargin::new(LeftMargin::INDENT, style))
    }

}

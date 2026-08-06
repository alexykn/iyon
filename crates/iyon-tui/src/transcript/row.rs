//! Transcript logical-row model.
//!
//! A transcript is a sequence of logical rows, each of which may carry a styled
//! *hanging indent* (a left margin). Indentation is a first-class field here
//! rather than literal spaces baked into span content, so the wrap step can
//! re-apply the margin to every wrapped physical row and wrap within the
//! remaining content width instead of counting the indent against it.

use ratatui::style::Style;
use ratatui::text::Line;

/// A styled margin for a logical row, re-applied to every wrapped physical row
/// that it produces. `left` is the hanging-indent width in display columns, `right`
/// the equal inset on the other side (so tool blocks read as symmetric callouts);
/// `style` is the style of the left-margin span (normally the same as the row's
/// content style so the indent reads as part of the line). Content wraps within
/// `terminal_width - left - right`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LeftMargin {
    pub(crate) left: u16,
    pub(crate) right: u16,
    pub(crate) style: Style,
}

impl LeftMargin {
    /// The standard indent for tool bullet/title/body rows.
    pub(crate) const INDENT: u16 = 2;

    /// No indent (column 0).
    pub(crate) const ZERO: LeftMargin = LeftMargin::new(0, 0, Style::reset());

    pub(crate) const fn new(left: u16, right: u16, style: Style) -> Self {
        Self { left, right, style }
    }
}

/// A single logical row of the transcript: display content plus an optional
/// styled hanging indent. Content excludes the indent; the margin is applied at
/// wrap time so every physical row (including wrapped continuation lines) keeps
/// the same left margin.
///
/// Bulleted header rows additionally carry a `first_prefix` (e.g. `"\u{25cf} "`)
/// that occupies the margin columns on the FIRST physical line, so a wrapped
/// bulleted row aligns its continuation lines under the text after the bullet
/// rather than under the bullet dot itself.
#[derive(Debug, Clone)]
pub(crate) struct TranscriptRow {
    pub(crate) line: Line<'static>,
    pub(crate) margin: LeftMargin,
    pub(crate) first_prefix: String,
}

impl TranscriptRow {
    /// A row at column 0 (no indent).
    pub(crate) fn new(line: Line<'static>) -> Self {
        Self {
            line,
            margin: LeftMargin::ZERO,
            first_prefix: String::new(),
        }
    }

    /// A row with an explicit hanging indent.
    pub(crate) fn with_margin(line: Line<'static>, margin: LeftMargin) -> Self {
        Self {
            line,
            margin,
            first_prefix: String::new(),
        }
    }

    /// An empty row (used for inter-block pacing).
    pub(crate) fn blank() -> Self {
        Self::new(Line::from(""))
    }

    /// A row indented by [`LeftMargin::INDENT`] columns whose margin and content share `style`.
    pub(crate) fn indented(text: impl Into<String>, style: Style) -> Self {
        Self::with_margin(
            Line::styled(text.into(), style),
            LeftMargin::new(LeftMargin::INDENT, LeftMargin::INDENT, style),
        )
    }

    /// A bulleted header row. The bullet (`"\u{25cf} "`) occupies the
    /// hanging-indent columns on the first line; wrapped continuation lines
    /// align under the text that follows the bullet, not under the bullet itself.
    pub(crate) fn bullet(text: impl Into<String>, style: Style) -> Self {
        Self {
            line: Line::styled(text.into(), style),
            margin: LeftMargin::new(LeftMargin::INDENT, LeftMargin::INDENT, style),
            first_prefix: "\u{25cf} ".to_string(),
        }
    }
}

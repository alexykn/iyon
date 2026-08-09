//! Manual layout and text compiler regressions.

use super::*;
use crate::physical::{PhysicalColor, PhysicalRow, PhysicalStyle};
use crate::presentation::api::style::{BorderSpec, BorderStyle, OverflowIndicator, TextAttribute};
use crate::presentation::ir::ViewKind;
use crate::presentation::stream::StreamRowCommit;
use crate::presentation::{
    ColorSpec, Decoration, ExactTerminator, HorizontalAlign, Insets, IntoView, ProjectedText,
    ProjectedTextLayout, ProjectedTextRun, RowChild, StreamNode, StreamOffset, StreamRange,
    StreamView, StyleSpec, TextSpan, ThemeKey, View, WidthRule, WrapMode,
};
use crate::theme;

fn range(start: u64, end: u64) -> StreamRange {
    StreamRange::new(StreamOffset::new(start), StreamOffset::new(end))
}

fn text(row: &PhysicalRow) -> String {
    row.plain_text()
}

fn style(color: &str) -> StyleSpec {
    StyleSpec {
        foreground: Some(ColorSpec::Theme(ThemeKey::from(color))),
        ..StyleSpec::default()
    }
}

fn tool_view(body: &str) -> View {
    View::row(
        vec![
            RowChild::content(
                View::text("●")
                    .no_wrap()
                    .style(style("tool.running"))
                    .into_view(),
            ),
            RowChild::flex(
                View::text(body)
                    .style(style("text.default"))
                    .width(WidthRule::Fill)
                    .into_view(),
            ),
        ],
        1,
    )
}

fn assert_block_shape(block: &LayoutBlock, width: u16, height: usize) {
    assert_eq!(block.width, width);
    assert_eq!(block.rows.len(), height);
}

fn empty_vertical() -> View {
    View::vertical(|_| {})
}

mod flow;
mod projected;
mod style;
mod text;

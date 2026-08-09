//! Backend-neutral intrinsic measurement for the manual layout engine.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{
    geometry::Size,
    presentation::{
        WrapMode,
        ir::{TextView, View, ViewKind},
    },
};

pub(crate) fn intrinsic_size(view: &View, width: u16) -> Size {
    let decoration = &view.decoration;
    let border = u16::from(decoration.border.is_some());
    let horizontal = border
        .saturating_mul(2)
        .saturating_add(decoration.padding.left)
        .saturating_add(decoration.padding.right);
    let inner_width = width.saturating_sub(horizontal);
    let core = match &view.kind {
        ViewKind::Text(text) => text_intrinsic_size(text, inner_width),
        ViewKind::Spacer { rows } => Size::new(0, *rows),
        ViewKind::Container(container) => intrinsic_size(&container.child, inner_width),
        ViewKind::ClampRows(clamp) => {
            let mut size = intrinsic_size(&clamp.child, inner_width);
            size.height = size.height.min(clamp.max_rows);
            size
        }
        ViewKind::Column(column) => {
            let children = column
                .children
                .iter()
                .map(|child| intrinsic_size(&child.view, inner_width))
                .collect::<Vec<_>>();
            Size::new(
                children.iter().map(|size| size.width).max().unwrap_or(0),
                children
                    .iter()
                    .map(|size| usize::from(size.height))
                    .sum::<usize>()
                    .saturating_add(
                        usize::from(column.gap).saturating_mul(children.len().saturating_sub(1)),
                    )
                    .min(usize::from(u16::MAX)) as u16,
            )
        }
        ViewKind::Row(row) => {
            let children = row
                .children
                .iter()
                .map(|child| intrinsic_size(&child.view, width))
                .collect::<Vec<_>>();
            Size::new(
                children
                    .iter()
                    .map(|size| size.width)
                    .sum::<u16>()
                    .saturating_add(column_gap(row.gap, children.len())),
                children.iter().map(|size| size.height).max().unwrap_or(0),
            )
        }
        ViewKind::ComponentSlot(_) => unreachable!("component slot reached measurement"),
    };

    Size::new(
        core.width
            .saturating_add(decoration.padding.left)
            .saturating_add(decoration.padding.right)
            .saturating_add(border.saturating_mul(2)),
        core.height
            .saturating_add(decoration.padding.top)
            .saturating_add(decoration.padding.bottom)
            .saturating_add(border.saturating_mul(2)),
    )
}

fn column_gap(gap: u16, count: usize) -> u16 {
    gap.saturating_mul(count.saturating_sub(1).min(usize::from(u16::MAX)) as u16)
}

pub(crate) fn text_intrinsic_size(text: &TextView, width: u16) -> Size {
    let content = text
        .spans
        .iter()
        .map(|span| span.text.as_str())
        .collect::<String>();
    let intrinsic_width = content
        .split('\n')
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(0)
        .min(usize::from(u16::MAX)) as u16;
    let height = wrapped_line_count(&content, width, text.wrap);
    Size::new(intrinsic_width.min(width), height)
}

fn wrapped_line_count(text: &str, width: u16, mode: WrapMode) -> u16 {
    let width = usize::from(width).max(1);
    let mut rows = 0usize;
    for line in text.split('\n') {
        if line.is_empty() {
            rows += 1;
            continue;
        }
        if mode == WrapMode::NoWrap {
            rows += 1;
            continue;
        }
        let mut current = 0usize;
        for grapheme in line.graphemes(true) {
            let grapheme_width = UnicodeWidthStr::width(grapheme);
            if current > 0 && current.saturating_add(grapheme_width) > width {
                rows += 1;
                current = 0;
            }
            current = current.saturating_add(grapheme_width);
        }
        rows += 1;
    }
    rows.max(1).min(usize::from(u16::MAX)) as u16
}

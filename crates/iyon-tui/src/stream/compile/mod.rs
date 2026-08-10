//! Width-dependent stream compilation.

mod rows;
mod text;

use crate::physical::PhysicalRow;

use super::{
    coord::StreamOffset,
    node::{StreamNode, StreamView},
    projected::checkpoint_after_row,
};

pub(crate) use rows::{
    CompiledStream, CompiledStreamRow, StreamAtomicId, StreamRowAnchor, StreamRowTransfer,
};

/// Compiles a [`StreamView`] into physical lines with transfer metadata at the specified width.
///
/// Uses the unified presentation text compiler to guarantee that styles, Unicode
/// graphemes, and wrapping agreement remain identical between stream and view paths.
pub(crate) fn compile_stream(
    view: &StreamView,
    max_width: u16,
    stable_through: StreamOffset,
) -> CompiledStream {
    use crate::presentation::layout::ViewCompiler;

    let width = max_width.max(1);
    let compiler = ViewCompiler::default();
    let mut rows = Vec::new();
    let mut anchors = Vec::new();
    let mut transfer = Vec::new();
    let mut transferable_prefix_rows = 0;
    let mut blocked = false;
    let mut leading_zero_rows = true;
    let mut zero_row_prefix = None;
    let mut next_atomic_id = 0usize;

    for node in &view.nodes {
        match node {
            StreamNode::Text(text) => {
                let (_w, compiled_rows) =
                    compiler.compile_projected_text_with_metadata(text, width);
                let final_offset = text.owned_range().end;
                let row_count = compiled_rows.len();
                if row_count == 0 {
                    leading_zero_rows = false;
                    rows.push(PhysicalRow::empty());
                    anchors.push(StreamRowAnchor::Checkpoint(text.content_range.start));
                    if !blocked && final_offset <= stable_through {
                        transfer.push(StreamRowTransfer::Checkpoint(final_offset));
                        transferable_prefix_rows += 1;
                    } else {
                        blocked = true;
                        transfer.push(StreamRowTransfer::Blocked);
                    }
                } else {
                    leading_zero_rows = false;
                    let mut checkpoint = text.content_range.start;
                    for (r_idx, row) in compiled_rows.into_iter().enumerate() {
                        let is_last = r_idx + 1 == row_count;
                        let offset = if is_last {
                            final_offset
                        } else {
                            row.source_end.map_or(checkpoint, |relative| {
                                text.content_range
                                    .start
                                    .checked_add(relative as u64)
                                    .expect("stream row checkpoint overflow")
                            })
                        };
                        anchors.push(StreamRowAnchor::Checkpoint(checkpoint));
                        rows.push(row.row);
                        let boundary = checkpoint_after_row(text, offset);
                        checkpoint = boundary;
                        if !blocked && row.fits && boundary <= stable_through {
                            transfer.push(StreamRowTransfer::Checkpoint(boundary));
                            transferable_prefix_rows += 1;
                        } else {
                            blocked = true;
                            transfer.push(StreamRowTransfer::Blocked);
                        }
                    }
                }
            }
            StreamNode::Atomic { range, view } => {
                next_atomic_id += 1;
                let group = StreamAtomicId(next_atomic_id);
                let layout = compiler.compile(view, width);
                let all_safe =
                    !blocked && range.end <= stable_through && layout.physically_complete;
                if layout.rows.is_empty() {
                    if leading_zero_rows && all_safe {
                        zero_row_prefix = Some(range.end);
                    } else {
                        blocked = true;
                    }
                    continue;
                }
                leading_zero_rows = false;

                for (row_index, row) in layout.rows.into_iter().enumerate() {
                    anchors.push(StreamRowAnchor::Atomic {
                        range: *range,
                        row: row_index,
                    });
                    rows.push(row);
                    if all_safe {
                        transfer.push(StreamRowTransfer::Atomic {
                            group,
                            source_end: range.end,
                        });
                        transferable_prefix_rows += 1;
                    } else {
                        blocked = true;
                        transfer.push(StreamRowTransfer::Blocked);
                    }
                }
            }
        }
    }

    let rows = if max_width == 0 {
        rows.into_iter().map(|_| PhysicalRow::empty()).collect()
    } else {
        rows
    };
    let rows = rows
        .into_iter()
        .zip(anchors)
        .zip(transfer)
        .map(|((physical, anchor), transfer)| CompiledStreamRow {
            physical,
            anchor,
            transfer,
        })
        .collect();
    CompiledStream {
        rows,
        transferable_prefix_rows,
        zero_row_prefix,
    }
}
